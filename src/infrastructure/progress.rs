//! Progress and cooperative controls shared with background filesystem workers.
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
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

#[derive(Clone, Debug)]
pub struct Conflict {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub identical: bool,
    pub source_bytes: u64,
    pub destination_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resolution {
    Skip,
    KeepBoth,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub phase: Phase,
    pub completed: u64,
    pub total: Option<u64>,
    pub files: u64,
    pub files_done: u64,
    pub bytes: u64,
    pub bytes_done: u64,
    pub paused: bool,
    pub cancelled: bool,
    pub conflict: Option<Conflict>,
    pub remaining: Option<Duration>,
}

struct State {
    snapshot: Snapshot,
    choice: Option<Resolution>,
    all: Option<Resolution>,
    started: Instant,
    elapsed: Duration,
    verifying: bool,
    verification_touched: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            snapshot: Snapshot {
                phase: Phase::Preparing,
                completed: 0,
                total: None,
                files: 0,
                files_done: 0,
                bytes: 0,
                bytes_done: 0,
                paused: false,
                cancelled: false,
                conflict: None,
                remaining: None,
            },
            choice: None,
            all: None,
            started: Instant::now(),
            elapsed: Duration::ZERO,
            verifying: false,
            verification_touched: false,
        }
    }
}

#[derive(Clone, Default)]
pub struct Progress(Arc<(Mutex<State>, Condvar)>);

impl Progress {
    pub fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub fn snapshot(&self) -> Snapshot {
        let state = self.0.0.lock().unwrap();
        let mut snapshot = state.snapshot.clone();
        let elapsed = state.elapsed
            + if snapshot.paused {
                Duration::ZERO
            } else {
                state.started.elapsed()
            };
        if snapshot.bytes_done > 0 && elapsed.as_secs_f64() > 0.5 {
            let seconds = elapsed.as_secs_f64()
                * snapshot.bytes.saturating_sub(snapshot.bytes_done) as f64
                / snapshot.bytes_done as f64;
            snapshot.remaining = Some(Duration::from_secs_f64(seconds.min(365. * 86400.)));
        }
        snapshot
    }

    pub fn begin(&self, phase: Phase, total: Option<u64>) {
        let mut state = self.0.0.lock().unwrap();
        if matches!(phase, Phase::Copying | Phase::Moving) {
            state.started = Instant::now();
            state.elapsed = Duration::ZERO;
        }
        state.snapshot.phase = phase;
        state.snapshot.completed = 0;
        state.snapshot.total = total;
    }

    pub fn advance(&self, amount: u64) {
        let mut state = self.0.0.lock().unwrap();
        state.snapshot.completed = state.snapshot.completed.saturating_add(amount);
    }

    pub fn totals(&self, files: u64, bytes: u64) {
        let mut state = self.0.0.lock().unwrap();
        state.snapshot.files = files;
        state.snapshot.bytes = bytes;
        state.started = Instant::now();
        state.elapsed = Duration::ZERO;
    }

    pub fn transferred(&self, bytes: u64, files: u64) {
        let mut state = self.0.0.lock().unwrap();
        state.snapshot.bytes_done = state.snapshot.bytes_done.saturating_add(bytes);
        state.snapshot.files_done = state.snapshot.files_done.saturating_add(files);
    }

    pub fn toggle_pause(&self) {
        let mut state = self.0.0.lock().unwrap();
        if state.snapshot.paused {
            state.started = Instant::now();
        } else {
            let elapsed = state.started.elapsed();
            state.elapsed += elapsed;
        }
        state.snapshot.paused = !state.snapshot.paused;
        self.0.1.notify_all();
    }

    pub fn cancel(&self) {
        self.0.0.lock().unwrap().snapshot.cancelled = true;
        self.0.1.notify_all();
    }

    pub fn checkpoint(&self) -> io::Result<()> {
        let mut state = self.0.0.lock().unwrap();
        // Finalize undo even after cancellation; partial successes remain undoable.
        while state.snapshot.phase != Phase::Finishing
            && !state.snapshot.cancelled
            && state.snapshot.paused
        {
            state = self.0.1.wait(state).unwrap();
        }
        if state.snapshot.phase != Phase::Finishing && state.snapshot.cancelled {
            Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Operation cancelled",
            ))
        } else {
            Ok(())
        }
    }

    pub fn resolve(&self, resolution: Resolution, all: bool) {
        let mut state = self.0.0.lock().unwrap();
        state.choice = Some(resolution);
        if all {
            state.all = Some(resolution);
        }
        self.0.1.notify_all();
    }

    pub fn conflict(&self, conflict: Conflict) -> io::Result<Resolution> {
        self.checkpoint()?;
        let mut state = self.0.0.lock().unwrap();
        if let Some(choice) = state.all {
            return Ok(choice);
        }
        state.snapshot.conflict = Some(conflict);
        while state.choice.is_none() && !state.snapshot.cancelled {
            state = self.0.1.wait(state).unwrap();
        }
        state.snapshot.conflict = None;
        state
            .choice
            .take()
            .ok_or_else(|| io::Error::new(io::ErrorKind::Interrupted, "Operation cancelled"))
    }

    pub fn configure_verification(&self, enabled: bool) {
        let mut state = self.0.0.lock().unwrap();
        if !state.verification_touched {
            state.verifying = enabled;
        }
    }

    pub fn set_verification(&self, enabled: bool) {
        let mut state = self.0.0.lock().unwrap();
        state.verification_touched = true;
        state.verifying = enabled;
    }
    pub fn verification(&self) -> bool {
        self.0.0.lock().unwrap().verifying
    }
}

// Include an item unit so empty files, directories and links also make progress.
pub fn weight(path: &Path) -> io::Result<u64> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error),
    };
    let mut total: u64 = 1;
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            total = total.saturating_add(weight(&entry?.path())?);
        }
    } else if metadata.is_file() {
        total = total.saturating_add(metadata.len());
    }
    Ok(total)
}
