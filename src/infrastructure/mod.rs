use std::{fs::File, io};
// The Windows `sync_file` fallback opens through `fs::OpenOptions`; the unix
// path uses `File::open`, so the plain `fs` import is windows-only.
#[cfg(windows)]
use std::fs;
pub(crate) mod archive;
pub(crate) mod editor_io;
pub(crate) mod image_edit;
pub(crate) mod layout;
pub(crate) mod media;
pub(crate) mod operations;
pub(crate) mod progress;
pub(crate) mod queue;
pub(crate) mod recent;
pub(crate) mod search;
pub(crate) mod ssh;
pub(crate) mod storage;
pub(crate) mod undo;
pub(crate) mod workspaces;

/// Non-blocking exclusive lock over an open file, so two Virial windows cannot
/// interleave journals or undo history. `busy` becomes the user-facing error.
#[cfg(unix)]
pub(crate) fn try_lock_exclusive(file: &File, busy: &'static str) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        let error = io::Error::last_os_error();
        if matches!(error.kind(), io::ErrorKind::WouldBlock) {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, busy));
        }
        return Err(error);
    }
    Ok(())
}

#[cfg(windows)]
pub(crate) fn try_lock_exclusive(file: &File, busy: &'static str) -> io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::{
        Foundation::ERROR_LOCK_VIOLATION,
        Storage::FileSystem::{LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY, LockFileEx},
        System::IO::OVERLAPPED,
    };
    let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
    let ok = unsafe {
        LockFileEx(
            file.as_raw_handle(),
            LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
            0,
            u32::MAX,
            u32::MAX,
            &mut overlapped,
        )
    };
    if ok != 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(ERROR_LOCK_VIOLATION as i32) {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, busy));
    }
    Err(error)
}

// Freedesktop-trash module: Linux only; a portable stub serves other targets.
#[cfg(unix)]
pub(crate) mod trash;
#[cfg(not(unix))]
pub(crate) mod trash_stub {
    use crate::domain::models::Entry;
    use std::{io, path::Path, path::PathBuf};

    pub(crate) fn read(_data: &Path) -> io::Result<Vec<Entry>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Trash is not supported on this platform yet",
        ))
    }

    pub(crate) fn empty(_data: &Path) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Emptying Trash is not supported on this platform yet",
        ))
    }

    pub(crate) fn restore(_data: &Path, _paths: &[PathBuf]) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Trash restore is not supported on this platform yet",
        ))
    }
}
#[cfg(not(unix))]
pub(crate) use trash_stub as trash;

#[cfg(all(test, windows))]
#[path = "../../tests/infrastructure/queue_windows.rs"]
mod queue_windows_tests;

#[cfg(test)]
#[path = "../../tests/infrastructure/trash_portable.rs"]
mod trash_portable_tests;

#[cfg(test)]
#[path = "../../tests/infrastructure/performance.rs"]
mod performance;

/// Flush a directory entry so renames inside it survive a crash. Windows
/// cannot open directories as files; there the best-effort no-op is correct
/// because NTFS metadata journaling already covers rename persistence.
pub(crate) fn sync_directory(directory: &std::path::Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        File::open(directory)?.sync_all()
    }
    #[cfg(windows)]
    {
        let _ = directory;
        Ok(())
    }
}
/// Flush file data to storage. Windows requires a writable handle for
/// FlushFileBuffers; opening for reading there would fail with Access Denied.
pub(crate) fn sync_file(file: &std::path::Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        File::open(file)?.sync_all()
    }
    #[cfg(windows)]
    {
        fs::OpenOptions::new().write(true).open(file)?.sync_all()
    }
}

pub mod compression;
