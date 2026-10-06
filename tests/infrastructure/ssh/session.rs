//! Tests for the pure mapping helpers of `infrastructure::ssh::session`.

use super::super::session::entry_from_parts;
use crate::domain::models::Entry;
use std::path::{Path, PathBuf};

#[test]
fn entries_are_parent_joined_and_carry_metadata() {
    let entry = entry_from_parts(Path::new("/srv/data"), "report.pdf", false, Some(2048));
    assert_eq!(entry.path, PathBuf::from("/srv/data/report.pdf"));
    assert_eq!(entry.name, "report.pdf");
    assert!(!entry.directory);
    assert_eq!(entry.bytes, Some(2048));
}

#[test]
fn directories_map_without_size() {
    let entry = entry_from_parts(Path::new("/srv/data"), "archive", true, None);
    assert!(entry.directory);
    assert_eq!(entry.bytes, None);
    assert_eq!(entry.kind(), "Folder");
}

#[test]
fn names_are_taken_verbatim_including_unicode() {
    let entry = entry_from_parts(Path::new("/"), "Rapport éàü.pdf", false, Some(1));
    assert_eq!(entry.name, "Rapport éàü.pdf");
    assert_eq!(entry.path, PathBuf::from("/Rapport éàü.pdf"));
}

#[test]
fn entry_kind_follows_extension() {
    let file = entry_from_parts(Path::new("/srv"), "notes.md", false, None);
    assert_eq!(file.kind(), "Document");
    let code = entry_from_parts(Path::new("/srv"), "main.rs", false, None);
    assert_eq!(code.kind(), "Source code");
}
