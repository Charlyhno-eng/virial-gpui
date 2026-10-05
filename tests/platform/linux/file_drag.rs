use super::*;
use std::{ffi::OsString, os::unix::ffi::OsStringExt};

#[test]
fn exports_files_and_folders_as_escaped_uri_list() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("a #é%\n.txt");
    let folder = temp.path().join("folder name");
    let bytes = temp
        .path()
        .join(OsString::from_vec(b"non-utf8-\xff".to_vec()));
    std::fs::write(&file, "file").unwrap();
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(&bytes, "file").unwrap();
    let paths = vec![file, folder, bytes];
    let payload = uri_list(&paths).unwrap();
    assert!(payload.ends_with("\r\n"));
    assert!(payload.contains("%20%23%C3%A9%25%0A.txt"));
    assert!(payload.contains("non-utf8-%FF"));
    assert_eq!(payload.matches("\r\n").count(), paths.len());
    let decoded: Vec<_> = payload
        .split_terminator("\r\n")
        .map(|uri| url::Url::parse(uri).unwrap().to_file_path().unwrap())
        .collect();
    assert_eq!(decoded, paths);
    assert!(paths.iter().all(|path| path.exists()));
}

#[test]
fn refuses_empty_relative_missing_and_virtual_archive_paths() {
    assert!(uri_list(&[]).is_none());
    assert!(uri_list(&[PathBuf::from("relative.txt")]).is_none());
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("existing.txt");
    std::fs::write(&file, "file").unwrap();
    assert!(uri_list(&[file, temp.path().join("missing")]).is_none());
    let zip = temp.path().join("archive.zip");
    std::fs::write(&zip, "zip").unwrap();
    assert!(uri_list(&[zip.join("member.txt")]).is_none());
}
