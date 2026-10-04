use super::*;
use std::os::unix::{ffi::OsStringExt, fs::symlink};

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
    assert_eq!(entries[0].bytes, Some(7));
    assert_eq!(entries[1].bytes, Some(7));
    assert_eq!(entries[2].name, "a-file");
    assert_eq!(entries[2].bytes, Some(5));
    assert_eq!(read_directory(&root, true).unwrap().len(), 6);
    assert!(read_directory(&root.join("a-file"), false).is_err());
    assert!(read_directory(&root.join("missing"), false).is_err());
    fs::remove_dir_all(root).unwrap();
}
