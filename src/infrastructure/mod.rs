pub(crate) mod archive;
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

    pub(crate) fn restore(_data: &Path, _paths: &[PathBuf]) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Trash restore is not supported on this platform yet",
        ))
    }
}
#[cfg(not(unix))]
pub(crate) use trash_stub as trash;

#[cfg(test)]
#[path = "../../tests/infrastructure/performance.rs"]
mod performance;
