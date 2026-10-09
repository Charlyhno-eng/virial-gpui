//! Cancellable, ranked global path search with a shared background catalog.
mod index;
use crate::domain::models::Entry;
pub(crate) use index::SearchIndex;
#[cfg(all(test, unix))]
use std::os::unix::fs::MetadataExt;
#[cfg(test)]
use std::{
    collections::{HashSet, VecDeque},
    fs,
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

fn subsequence<T: PartialEq>(
    mut letters: impl Iterator<Item = T>,
    path: impl Iterator<Item = T>,
) -> bool {
    let Some(mut next) = letters.next() else {
        return true;
    };
    for ch in path {
        if ch == next {
            match letters.next() {
                Some(letter) => next = letter,
                None => return true,
            }
        }
    }
    false
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
            // ASCII bytes cannot occur inside a UTF-8 multibyte character.
            // Common queries can skip decoding the path; Unicode keeps char matching.
            let matches = if term.is_ascii() {
                subsequence(term.bytes(), path.bytes())
            } else {
                subsequence(term.chars(), path.chars())
            };
            if !matches {
                return None;
            }
            4
        };
    }
    Some(total)
}

// Reference traversal for ranking regressions and the cold filesystem benchmark.
#[cfg(test)]
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
    // Finish each root in priority order, visiting siblings before deeper trees.
    // A large cache or dependency tree must not delay nearby project folders.
    let mut roots = roots.into_iter();
    let mut pending = VecDeque::new();
    let mut visited = HashSet::new();
    let mut matches: Vec<(usize, String, Entry)> = Vec::new();
    let mut skipped = 0;
    let mut last_update = Instant::now();
    let mut published_match = false;
    while let Some(directory) = pending.pop_front().or_else(|| roots.next()) {
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
        #[cfg(unix)]
        let identity = (metadata.dev(), metadata.ino());
        #[cfg(windows)]
        let identity = (metadata.len(), 0u64);
        if !visited.insert(identity) {
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
                pending.push_back(path.clone());
            }
            let path_key = path.to_string_lossy().to_lowercase();
            if let Some(rank) = score(&name.to_lowercase(), &path_key, &terms) {
                // Do not descend through symlinks, but allow opening a linked folder.
                let directory = kind.is_dir() || (kind.is_symlink() && path.is_dir());
                let position = matches
                    .binary_search_by(|candidate| {
                        (candidate.0, &candidate.1).cmp(&(rank, &path_key))
                    })
                    .unwrap_or_else(|position| position);
                if position < RESULT_LIMIT {
                    matches.insert(
                        position,
                        (
                            rank,
                            path_key,
                            Entry {
                                path,
                                name,
                                directory,
                                bytes: None,
                            },
                        ),
                    );
                    matches.truncate(RESULT_LIMIT);
                }
            }
            if (!published_match && !matches.is_empty())
                || last_update.elapsed() >= Duration::from_millis(100)
            {
                if !publish(SearchResults {
                    entries: matches.iter().map(|(_, _, entry)| entry.clone()).collect(),
                    skipped,
                    finished: false,
                }) {
                    return;
                }
                published_match |= !matches.is_empty();
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

#[cfg(all(test, unix))]
#[path = "../../tests/infrastructure/search.rs"]
mod tests;
