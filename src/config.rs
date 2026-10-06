//! Startup configuration from the command line and environment.
use std::path::PathBuf;

pub(crate) fn initial_path() -> PathBuf {
    let path = match std::env::args_os().nth(1) {
        Some(argument) => PathBuf::from(argument),
        None => match std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            Some(home) => PathBuf::from(home),
            None => match (std::env::var_os("HOMEDRIVE"), std::env::var_os("HOMEPATH")) {
                (Some(drive), Some(path)) => PathBuf::from(drive).join(path),
                _ => PathBuf::from("/"),
            },
        },
    };
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(path)
    };
    path.canonicalize().unwrap_or(path)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_path_returns_an_absolute_path() {
        // The portable fallback chain (argument, HOME, USERPROFILE,
        // HOMEDRIVE+HOMEPATH, /) must always yield an absolute path.
        assert!(initial_path().is_absolute());
    }
}