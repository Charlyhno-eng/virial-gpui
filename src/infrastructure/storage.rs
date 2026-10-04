use crate::domain::models::Entry;
use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};
use std::{
    collections::HashSet,
    fs, io,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub fn read_directory(path: &Path, hidden: bool) -> io::Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for item in fs::read_dir(path)? {
        let item = item?;
        if !hidden && item.file_name().as_bytes().starts_with(b".") {
            continue;
        }
        // Follow directory symlinks, but retain broken links in the listing.
        let metadata = fs::metadata(item.path()).ok();
        let directory = metadata.as_ref().is_some_and(|m| m.is_dir());
        let bytes = metadata.and_then(|metadata| {
            if metadata.is_file() {
                Some(metadata.len())
            } else {
                None
            }
        });
        entries.push(Entry {
            path: item.path(),
            name: item.file_name().to_string_lossy().into_owned(),
            directory,
            bytes,
        });
    }
    // Compute case-insensitive keys once instead of allocating on every comparison.
    entries.sort_by_cached_key(|entry| {
        (
            !entry.directory,
            entry.name.to_lowercase(),
            entry.path.clone(),
        )
    });
    Ok(entries)
}

pub fn directory_size(path: &Path, cancelled: &AtomicBool) -> io::Result<u64> {
    fn check_cancelled(cancelled: &AtomicBool) -> io::Result<()> {
        if cancelled.load(Ordering::Relaxed) {
            Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Size scan cancelled",
            ))
        } else {
            Ok(())
        }
    }

    // Resolve a listed directory symlink once, then ignore symlinks encountered
    // below it so they cannot create cycles or count the same tree repeatedly.
    check_cancelled(cancelled)?;
    let mut pending = vec![fs::canonicalize(path)?];
    let mut visited = HashSet::new();
    let mut total = 0u64;
    while let Some(path) = pending.pop() {
        check_cancelled(cancelled)?;
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_dir() || !visited.insert((metadata.dev(), metadata.ino())) {
            continue;
        }
        for item in fs::read_dir(path)? {
            check_cancelled(cancelled)?;
            let item = item?;
            let kind = item.file_type()?;
            if kind.is_dir() {
                pending.push(item.path());
            } else if kind.is_file() {
                let metadata = item.metadata()?;
                if metadata.is_file() {
                    total = total.saturating_add(metadata.len());
                }
            }
        }
    }
    check_cancelled(cancelled)?;
    Ok(total)
}

#[cfg(test)]
#[path = "../../tests/infrastructure/storage.rs"]
mod tests;
