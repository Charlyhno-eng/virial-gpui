//! Startup configuration from the command line and environment.
use std::path::PathBuf;

pub(crate) fn initial_path() -> PathBuf {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("/"));
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(path)
    };
    path.canonicalize().unwrap_or(path)
}
