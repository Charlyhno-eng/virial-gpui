use crate::domain::models::Entry;
#[cfg(unix)]
use std::{
    ffi::CString,
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
};
use std::{
    collections::HashSet,
    ffi::CString,
    fs, io,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub fn read_directory(path: &Path, hidden: bool) -> io::Result<Vec<Entry>> {
    if let Some((archive, member)) = super::archive::split(path) {
        return super::archive::read_directory(&archive, &member, hidden);
    }
    let mut entries = Vec::new();
    for item in fs::read_dir(path)? {
        let item = item?;
        #[cfg(unix)]
        let skip = !hidden && item.file_name().as_bytes().starts_with(b".");
        #[cfg(not(unix))]
        let skip = !hidden
            && item.file_name().to_string_lossy().starts_with('.')
            && item.file_name() != "."
            && item.file_name() != "..";
        if skip {
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

/// Count a folder without allocating entries, reading metadata or sorting names.
pub fn directory_entry_count(path: &Path, hidden: bool) -> io::Result<usize> {
    if let Some((archive, member)) = super::archive::split(path) {
        return super::archive::read_directory(&archive, &member, hidden)
            .map(|entries| entries.len());
    }
    fs::read_dir(path)?.try_fold(0, |count, item| {
        let item = item?;
        #[cfg(unix)]
        let hidden_entry = item.file_name().as_bytes().starts_with(b".");
        #[cfg(not(unix))]
        let hidden_entry = item
            .file_name()
            .to_string_lossy()
            .starts_with('.')
            && item.file_name() != "."
            && item.file_name() != "..";
        Ok(count + usize::from(hidden || !hidden_entry))
    })
}

pub fn directory_size(path: &Path, cancelled: &AtomicBool) -> io::Result<u64> {
    if let Some((archive, member)) = super::archive::split(path) {
        if cancelled.load(Ordering::Relaxed) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        return super::archive::directory_size(&archive, &member);
    }
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
        #[cfg(unix)]
        let identity = (metadata.dev(), metadata.ino());
        #[cfg(windows)]
        let identity = (metadata.len(), 0u64);
        if !metadata.is_dir() || !visited.insert(identity) {
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

#[cfg(windows)]
pub fn available_space(path: &Path) -> io::Result<u64> {
    use std::os::windows::ffi::OsStrExt as _;
    let mut wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    if wide.len() > 4 {
        while wide.len() > 2 && *wide.iter().rev().nth(1).unwrap() == u16::from(b'\\') {
            wide.remove(wide.len() - 2);
        }
    }
    let mut free: u64 = 0;
    let mut total: u64 = 0;
    let mut unused: u64 = 0;
    // SAFETY: `wide` is NUL-terminated, the targets are writable u64s.
    let ok = unsafe {
        windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut unused,
            &mut total,
            &mut free,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(free)
}
#[cfg(unix)]
pub fn available_space(path: &Path) -> io::Result<u64> {
    let path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Path contains a NUL byte"))?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: `path` is NUL-terminated and `stats` points to writable storage.
    if unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `statvfs` initialized `stats` when it returned successfully.
    let stats = unsafe { stats.assume_init() };
    let block_size = if stats.f_frsize > 0 {
        stats.f_frsize
    } else {
        stats.f_bsize
    };
    let bytes = (stats.f_bavail as u128).saturating_mul(block_size as u128);
    Ok(bytes.min(u64::MAX as u128) as u64)
}

#[cfg(test)]
#[path = "../../tests/infrastructure/storage.rs"]
mod tests;
