//! Discover common Linux locations once, including localized XDG user directories.
use std::{
    fs,
    path::{Path, PathBuf},
};

pub struct Place {
    pub label: &'static str,
    pub icon: &'static str,
    pub path: PathBuf,
}

pub fn discover(home: &Path) -> Vec<Place> {
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".config"));
    let config = fs::read_to_string(config_home.join("user-dirs.dirs")).unwrap_or_default();
    let mut places = vec![Place {
        label: "Home",
        icon: "home",
        path: home.into(),
    }];
    for (label, icon, key) in [
        ("Desktop", "drive", "DESKTOP"),
        ("Documents", "file", "DOCUMENTS"),
        ("Downloads", "download", "DOWNLOAD"),
        ("Pictures", "image", "PICTURES"),
        ("Music", "music", "MUSIC"),
        ("Videos", "image", "VIDEOS"),
    ] {
        let path = configured_directory(&config, key, home).unwrap_or_else(|| home.join(label));
        if path != home && path.is_dir() {
            places.push(Place { label, icon, path });
        }
    }
    places.push(Place {
        label: "File System",
        icon: "drive",
        path: "/".into(),
    });
    places
}

fn configured_directory(config: &str, key: &str, home: &Path) -> Option<PathBuf> {
    let key = format!("XDG_{key}_DIR");
    config.lines().find_map(|line| {
        let (name, value) = line.trim().split_once('=')?;
        if name.trim() != key {
            return None;
        }
        let value = value.trim().strip_prefix('"')?.strip_suffix('"')?;
        let path = if value == "$HOME" {
            home.into()
        } else if let Some(relative) = value.strip_prefix("$HOME/") {
            home.join(relative)
        } else {
            PathBuf::from(value)
        };
        path.is_absolute().then_some(path)
    })
}

#[cfg(test)]
#[path = "../../../tests/platform/linux/places.rs"]
mod tests;
