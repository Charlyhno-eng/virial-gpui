//! Shared progress for a background transfer and its undo snapshots.
use std::{
    fs, io,
    path::Path,
    sync::{Arc, Mutex},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Preparing,
    SavingUndo,
    Moving,
    Copying,
    Finishing,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Snapshot {
    pub phase: Phase,
    pub completed: u64,
    pub total: Option<u64>,
}

#[derive(Clone, Default)]
pub struct Progress(Arc<Mutex<Snapshot>>);

impl Progress {
    pub fn snapshot(&self) -> Snapshot {
        *self.0.lock().unwrap()
    }

    pub fn begin(&self, phase: Phase, total: Option<u64>) {
        *self.0.lock().unwrap() = Snapshot {
            phase,
            completed: 0,
            total,
        };
    }

    pub fn advance(&self, amount: u64) {
        let mut state = self.0.lock().unwrap();
        state.completed = state.completed.saturating_add(amount);
    }
}

// Include an item unit so empty files, directories and links also make progress.
pub fn weight(path: &Path) -> io::Result<u64> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error),
    };
    let mut total = 1;
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            total += weight(&entry?.path())?;
        }
    } else if metadata.is_file() {
        total += metadata.len();
    }
    Ok(total)
}
