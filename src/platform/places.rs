//! Portable fallback locations: derive from the current HOME-equivalent
//! directory. Windows keeps the same sidebar structure with well-known names.
use std::path::{Path, PathBuf};

pub struct Place {
    pub label: &'static str,
    pub icon: &'static str,
    pub path: PathBuf,
}

pub fn discover(home: &Path) -> Vec<Place> {
    let mut places = vec![Place {
        label: "Home",
        icon: "home",
        path: home.to_path_buf(),
    }];
    for (label, icon) in [
        ("Desktop", "drive"),
        ("Documents", "file"),
        ("Downloads", "download"),
        ("Pictures", "image"),
        ("Music", "music"),
        ("Videos", "video"),
    ] {
        let path = home.join(label);
        if path.is_dir() {
            places.push(Place { label, icon, path });
        }
    }
    places.push(Place {
        label: "File System",
        icon: "drive",
        path: home
            .ancestors()
            .last()
            .unwrap_or(Path::new("/"))
            .to_path_buf(),
    });
    places
}
