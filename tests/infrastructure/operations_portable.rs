use super::*;
use std::fs;

#[test]
fn rename_never_overwrites_an_existing_destination() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.txt");
    let destination = root.path().join("destination.txt");
    fs::write(&source, b"new").unwrap();
    fs::write(&destination, b"original").unwrap();
    let error = rename(&source, &destination).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    // The original destination is untouched.
    assert_eq!(fs::read(&destination).unwrap(), b"original");
    assert_eq!(fs::read(&source).unwrap(), b"new");
}

#[test]
fn rename_succeeds_when_the_destination_is_free() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.txt");
    let destination = root.path().join("destination.txt");
    fs::write(&source, b"payload").unwrap();
    rename(&source, &destination).unwrap();
    assert!(!source.exists());
    assert_eq!(fs::read(&destination).unwrap(), b"payload");
}

#[test]
fn named_path_rejects_traversal_empty_and_nested_names() {
    let root = tempfile::tempdir().unwrap();
    assert!(named_path(root.path(), "").is_err());
    assert!(named_path(root.path(), "../escape").is_err());
    assert!(named_path(root.path(), "a/b").is_err());
    assert!(named_path(root.path(), "a\\b").is_err() || cfg!(unix));
    assert!(named_path(root.path(), ".").is_err());
    assert!(named_path(root.path(), "..").is_err());
    let ok = named_path(root.path(), "plain.txt").unwrap();
    assert_eq!(ok, root.path().join("plain.txt"));
}
