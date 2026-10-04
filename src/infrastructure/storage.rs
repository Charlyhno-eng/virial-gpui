use crate::domain::models::Entry;
use std::os::unix::ffi::OsStrExt;
use std::{fs, io, path::Path};

pub fn read_directory(path: &Path, hidden: bool) -> io::Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for item in fs::read_dir(path)? {
        let item = item?;
        if !hidden && item.file_name().as_bytes().starts_with(b".") {
            continue;
        }
        // Follow directory symlinks, but retain broken links in the listing.
        let metadata = fs::metadata(item.path()).ok();
        entries.push(Entry {
            path: item.path(),
            name: item.file_name().to_string_lossy().into_owned(),
            directory: metadata.as_ref().is_some_and(|m| m.is_dir()),
            bytes: metadata.filter(|m| m.is_file()).map(|m| m.len()),
        });
    }
    entries.sort_by(|a, b| {
        b.directory
            .cmp(&a.directory)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.path.cmp(&b.path))
    });
    Ok(entries)
}

#[cfg(test)]
#[path = "../../tests/infrastructure/storage.rs"]
mod tests;
