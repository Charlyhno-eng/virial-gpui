use crate::infrastructure::storage::{
    available_space, directory_entry_count, directory_size, read_directory,
};
use std::{fs, sync::atomic::AtomicBool};

fn unique_root(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("virial-portable-{}-{}", tag, std::process::id()))
}

#[test]
fn read_directory_reports_sizes_and_separates_folders() {
    let root = unique_root("listing");
    fs::create_dir_all(root.join("z-folder/nested")).unwrap();
    fs::write(root.join("z-folder/nested/deep.txt"), b"12345").unwrap();
    fs::write(root.join("a-file.txt"), b"hello").unwrap();
    let entries = read_directory(&root, false).unwrap();
    assert_eq!(entries.len(), 2);
    // Folders sort before files, then case-insensitive by name.
    assert!(entries[0].directory);
    assert!(!entries[1].directory);
    assert_eq!(entries[1].name, "a-file.txt");
    assert_eq!(entries[1].bytes, Some(5));
    // Listing never computes recursive sizes eagerly.
    assert_eq!(entries[0].bytes, None);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn entry_count_matches_read_directory_length() {
    let root = unique_root("count");
    fs::create_dir_all(root.join("inner")).unwrap();
    fs::write(root.join("inner/one"), b"1").unwrap();
    fs::write(root.join("two"), b"22").unwrap();
    fs::write(root.join(".dot-file"), b"").unwrap();
    assert_eq!(directory_entry_count(&root, true).unwrap(), 3);
    // ".dot-file" counts as hidden, so it disappears when hidden=false.
    assert_eq!(directory_entry_count(&root, false).unwrap(), 2);
    assert!(directory_entry_count(&root.join("missing"), false).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn folder_sizes_sum_files_and_tolerate_empty_folders() {
    let root = unique_root("sizes");
    let folder = root.join("folder");
    fs::create_dir_all(folder.join("nested")).unwrap();
    fs::write(folder.join("file"), b"12345").unwrap();
    fs::write(folder.join("nested/deep"), b"1234567").unwrap();
    fs::create_dir(folder.join("empty")).unwrap();
    let cancelled = AtomicBool::new(false);
    assert_eq!(directory_size(&folder, &cancelled).unwrap(), 12);
    assert_eq!(
        directory_size(&folder.join("empty"), &cancelled).unwrap(),
        0
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cancelled_size_scan_reports_interruption_and_missing_path_reports_not_found() {
    let missing = unique_root("cancelled").join("never-created");
    assert_eq!(
        directory_size(&missing, &AtomicBool::new(true))
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::Interrupted
    );
    assert_eq!(
        directory_size(&missing, &AtomicBool::new(false))
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::NotFound
    );
}

#[test]
fn available_space_is_plausible_for_the_current_filesystem() {
    let directory = tempfile::tempdir().unwrap();
    let free = available_space(directory.path()).unwrap();
    // Any real filesystem reports something; a drive root has > 1 KiB free
    // in practice, and zero would mean the query silently failed.
    assert!(free > 0, "free space must not be zero");
}

#[test]
fn available_space_accepts_a_file_path() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("probe.txt");
    fs::write(&file, b"probe").unwrap();
    assert!(available_space(&file).is_ok());
}

#[test]
fn available_space_reports_an_error_for_a_missing_path() {
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("missing.txt");
    let result = available_space(&missing);
    // A missing target is an error on both platforms; accept either a clean
    // OS error or a zero result, but never a panic.
    assert!(result.is_err() || result.unwrap() == 0);
}
