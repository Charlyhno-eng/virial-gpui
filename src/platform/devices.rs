//! Portable fallback devices: no removable-volume backend on this target yet.
//! The sidebar simply shows no devices, and device actions report unsupported.
use futures_lite::future;
use std::{io, path::PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Volume {
    pub object: String,
    pub drive: String,
    pub device: PathBuf,
    pub label: String,
    pub size: u64,
    pub mountpoints: Vec<PathBuf>,
    pub can_power_off: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Open,
    Unmount,
    SafelyRemove,
}

pub(crate) fn discover() -> io::Result<Vec<Volume>> {
    Ok(Vec::new())
}

pub(crate) struct Monitor;

impl Monitor {
    pub(crate) async fn new() -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "No volume monitor on this platform yet",
        ))
    }

    pub(crate) async fn discover(&mut self) -> io::Result<Vec<Volume>> {
        discover()
    }

    pub(crate) async fn changed(&mut self) -> io::Result<()> {
        future::pending::<()>().await;
        Ok(())
    }
}

pub(crate) fn execute(_volume: &Volume, _action: Action) -> io::Result<Option<PathBuf>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Device operations are not supported on this platform yet",
    ))
}
