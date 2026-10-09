use crate::platform::{applications, file_drag, places};
use crate::ui::i18n::Language;
use std::{fs, path::PathBuf};

fn temp_home(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("virial-home-{}-{}", tag, std::process::id()))
}

#[test]
fn portable_places_include_home_and_only_existing_subfolders() {
    let home = temp_home("places");
    fs::create_dir_all(home.join("Documents")).unwrap();
    fs::create_dir_all(home.join("Downloads")).unwrap();
    // "Music" deliberately not created: it must be skipped.
    let discovered = places::discover(&home);
    let labels: Vec<&str> = discovered.iter().map(|place| place.label).collect();
    assert_eq!(labels.first(), Some(&"Home"));
    assert!(labels.contains(&"Documents"));
    assert!(labels.contains(&"Downloads"));
    assert!(!labels.contains(&"Music"));
    // The last entry is the filesystem root that contains home.
    let root = discovered.last().unwrap();
    assert!(home.starts_with(&root.path));
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn portable_places_never_panics_on_a_missing_home() {
    let home = temp_home("missing-home");
    // The directory does not exist; discovery must still return Home + root.
    let discovered = places::discover(&home);
    assert!(discovered.len() >= 2);
    assert_eq!(discovered[0].label, "Home");
}

#[cfg(not(target_os = "linux"))]
#[test]
fn portable_open_with_list_is_empty_but_callable() {
    let home = temp_home("apps");
    fs::create_dir_all(&home).unwrap();
    let installed = applications::installed(&home, Language::English);
    assert!(installed.is_empty());
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn file_drag_uri_list_rejects_virtual_and_missing_paths() {
    // A relative path models a ZIP member: it must cancel the native drag.
    let relative = PathBuf::from("archive.zip/member.txt");
    assert!(file_drag::uri_list(&[relative]).is_none());
    let missing = std::env::temp_dir().join("virial-drag-does-not-exist.txt");
    assert!(file_drag::uri_list(&[missing]).is_none());
    assert!(file_drag::uri_list(&[]).is_none());
}

#[cfg(unix)]
#[test]
fn file_drag_uri_list_encodes_absolute_paths_as_file_uris() {
    use std::os::unix::fs::symlink;
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("real.txt");
    fs::write(&file, b"data").unwrap();
    let uris = file_drag::uri_list(&[file]).unwrap();
    assert!(uris.starts_with("file://"));
    assert!(uris.ends_with("\r\n"));
    let missing = directory.path().join("removed.txt");
    fs::write(&missing, b"x").unwrap();
    fs::remove_file(&missing).unwrap();
    let _ = symlink(directory.path(), directory.path().join("ignored"));
    assert!(file_drag::uri_list(&[missing]).is_none());
}
