use crate::domain::models::Entry;
use std::os::unix::ffi::OsStrExt;
use std::{collections::HashSet, fs, io, path::Path};

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
            } else if directory {
                directory_size(&item.path()).ok()
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
    entries.sort_by(|a, b| {
        b.directory
            .cmp(&a.directory)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.path.cmp(&b.path))
    });
    Ok(entries)
}

fn directory_size(path: &Path) -> io::Result<u64> {
    fn visit(path: &Path, visited: &mut HashSet<std::path::PathBuf>) -> io::Result<u64> {
        let metadata = fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink() {
            return Ok(0);
        }
        if metadata.is_file() {
            return Ok(metadata.len());
        }
        if !metadata.is_dir() {
            return Ok(0);
        }

        let canonical = fs::canonicalize(path)?;
        if !visited.insert(canonical) {
            return Ok(0);
        }

        let mut total = 0u64;
        for item in fs::read_dir(path)? {
            total = total.saturating_add(visit(&item?.path(), visited)?);
        }
        Ok(total)
    }

    // Resolve a listed directory symlink once, then ignore symlinks encountered
    // below it so they cannot create cycles or count the same tree repeatedly.
    visit(&fs::canonicalize(path)?, &mut HashSet::new())
}

#[cfg(test)]
#[path = "../../tests/infrastructure/storage.rs"]
mod tests;
