//! Cancellable global path search. No external index or command is required.
use crate::domain::models::Entry;
use std::{
    collections::HashSet,
    fs,
    os::unix::fs::MetadataExt,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub(crate) const RESULT_LIMIT: usize = 100;

#[derive(Clone, Default)]
pub(crate) struct SearchResults {
    pub entries: Vec<Entry>,
    pub skipped: usize,
    pub finished: bool,
}

fn score(name: &str, path: &str, terms: &[String]) -> Option<usize> {
    let mut total = 0;
    for term in terms {
        total += if name == term {
            0
        } else if name.starts_with(term) {
            1
        } else if name.contains(term) {
            2
        } else if path.contains(term) {
            3
        } else {
            // Like a jump picker, tolerate missing letters while retaining order.
            let mut letters = term.chars();
            let mut next = letters.next();
            for ch in path.chars() {
                if next == Some(ch) {
                    next = letters.next();
                }
            }
            if next.is_some() {
                return None;
            }
            4
        };
    }
    Some(total)
}

pub(crate) fn search(
    roots: Vec<PathBuf>,
    query: &str,
    hidden: bool,
    cancelled: &AtomicBool,
    mut publish: impl FnMut(SearchResults) -> bool,
) {
    let terms: Vec<_> = query.split_whitespace().map(str::to_lowercase).collect();
    if terms.is_empty() || cancelled.load(Ordering::Relaxed) {
        return;
    }
    let mut pending = roots;
    let mut visited = HashSet::new();
    let mut matches: Vec<(usize, String, Entry)> = Vec::new();
    let mut skipped = 0;
    let mut last_update = Instant::now();
    while let Some(directory) = pending.pop() {
        if cancelled.load(Ordering::Relaxed) {
            return;
        }
        // Virtual kernel trees are not user files and can be unbounded.
        if ["/proc", "/sys", "/dev"]
            .iter()
            .any(|root| directory.starts_with(root))
        {
            continue;
        }
        let metadata = match fs::metadata(&directory) {
            Ok(metadata) => metadata,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        if !visited.insert((metadata.dev(), metadata.ino())) {
            continue;
        }
        let items = match fs::read_dir(&directory) {
            Ok(items) => items,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        for item in items {
            if cancelled.load(Ordering::Relaxed) {
                return;
            }
            let Ok(item) = item else {
                skipped += 1;
                continue;
            };
            let name = item.file_name().to_string_lossy().into_owned();
            if !hidden && name.starts_with('.') {
                continue;
            }
            let Ok(kind) = item.file_type() else {
                skipped += 1;
                continue;
            };
            let path = item.path();
            if kind.is_dir() {
                pending.push(path.clone());
            }
            let path_key = path.to_string_lossy().to_lowercase();
            if let Some(rank) = score(&name.to_lowercase(), &path_key, &terms) {
                // Do not descend through symlinks, but allow opening a linked folder.
                let directory = kind.is_dir() || (kind.is_symlink() && path.is_dir());
                matches.push((
                    rank,
                    path_key,
                    Entry {
                        path,
                        name,
                        directory,
                        bytes: None,
                    },
                ));
                matches.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
                matches.truncate(RESULT_LIMIT);
            }
            if last_update.elapsed() >= Duration::from_millis(100) {
                if !publish(SearchResults {
                    entries: matches.iter().map(|(_, _, entry)| entry.clone()).collect(),
                    skipped,
                    finished: false,
                }) {
                    return;
                }
                last_update = Instant::now();
            }
        }
    }
    if !cancelled.load(Ordering::Relaxed) {
        publish(SearchResults {
            entries: matches.into_iter().map(|(_, _, entry)| entry).collect(),
            skipped,
            finished: true,
        });
    }
}

#[cfg(test)]
#[path = "../../tests/infrastructure/search.rs"]
mod tests;
