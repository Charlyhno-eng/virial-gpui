use super::*;
#[cfg(unix)]
use std::os::unix::{ffi::OsStringExt, fs::symlink};
#[cfg(unix)]
use std::ffi::CString;
use libc;
#[cfg(windows)]
use std::os::windows::fs::{symlink_dir as symlink_dir_win, symlink_file as symlink_file_win};
#[cfg(windows)]
fn symlink<P: AsRef<std::path::Path>, Q: AsRef<std::path::Path>>(
    original: P,
    link: Q,
) -> std::io::Result<()> {
    if std::fs::metadata(&original).map(|m| m.is_dir()).unwrap_or(false) {
        symlink_dir_win(original, link)
    } else {
        symlink_file_win(original, link)
    }
}

#[test]
fn lists_directories_first_and_handles_hidden_files_and_links() {
    let root = std::env::temp_dir().join(format!("virial-test-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    fs::create_dir(root.join("z-folder")).unwrap();
    fs::write(root.join("z-folder").join("nested-file"), b"1234567").unwrap();
    fs::write(root.join("a-file"), b"hello").unwrap();
    fs::write(root.join(".hidden"), b"").unwrap();
    fs::write(root.join(std::ffi::OsString::from_vec(vec![0xff])), b"").unwrap();
    symlink(root.join("z-folder"), root.join("linked-folder")).unwrap();
    symlink(root.join("missing"), root.join("broken-link")).unwrap();
    let entries = read_directory(&root, false).unwrap();
    assert_eq!(entries.len(), 5);
    assert!(entries[0].directory && entries[1].directory);
    // Listing must not wait for recursive sizes; a separate worker fills these in.
    assert_eq!(entries[0].bytes, None);
    assert_eq!(entries[1].bytes, None);
    assert_eq!(entries[2].name, "a-file");
    assert_eq!(entries[2].bytes, Some(5));
    assert_eq!(read_directory(&root, true).unwrap().len(), 6);
    assert!(read_directory(&root.join("a-file"), false).is_err());
    assert!(read_directory(&root.join("missing"), false).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn reports_available_space_for_an_existing_filesystem() {
    let directory = tempfile::tempdir().unwrap();
    assert!(available_space(directory.path()).is_ok());
}

#[test]
fn folder_sizes_include_hidden_files_but_skip_nested_links_and_special_files() {
    let root = std::env::temp_dir().join(format!("virial-size-test-{}", std::process::id()));
    let folder = root.join("folder");
    fs::create_dir_all(folder.join("nested")).unwrap();
    fs::create_dir(folder.join("empty")).unwrap();
    fs::write(folder.join("file"), b"12345").unwrap();
    fs::write(folder.join(".hidden"), b"123").unwrap();
    fs::write(folder.join("nested/file"), b"1234567").unwrap();
    symlink(&folder, root.join("linked-folder")).unwrap();
    symlink(&folder, folder.join("nested/cycle")).unwrap();
    symlink(folder.join("file"), folder.join("file-link")).unwrap();
    symlink(folder.join("nested"), folder.join("folder-link")).unwrap();
    symlink(root.join("missing"), folder.join("broken-link")).unwrap();
    let pipe = std::ffi::CString::new(folder.join("pipe").as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(pipe.as_ptr(), 0o600) }, 0);
    let cancelled = AtomicBool::new(false);
    assert_eq!(directory_size(&folder, &cancelled).unwrap(), 15);
    assert_eq!(
        directory_size(&root.join("linked-folder"), &cancelled).unwrap(),
        15
    );
    assert_eq!(
        directory_size(&folder.join("empty"), &cancelled).unwrap(),
        0
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn folder_sizes_handle_deep_trees_without_recursive_calls() {
    let root = std::env::temp_dir().join(format!("virial-deep-size-test-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let mut path = root.clone();
    for _ in 0..180 {
        path.push("d");
        fs::create_dir(&path).unwrap();
        fs::write(path.join("file"), b"123").unwrap();
    }
    assert_eq!(directory_size(&root, &AtomicBool::new(false)).unwrap(), 540);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cancelled_folder_sizes_stop_before_accessing_the_filesystem() {
    let missing =
        std::env::temp_dir().join(format!("virial-missing-size-test-{}", std::process::id()));
    assert_eq!(
        directory_size(&missing, &AtomicBool::new(true))
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(
        directory_size(&missing, &AtomicBool::new(false))
            .unwrap_err()
            .kind(),
        io::ErrorKind::NotFound
    );
}
