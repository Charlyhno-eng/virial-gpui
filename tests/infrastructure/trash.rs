use super::*;
use std::os::unix::{ffi::OsStrExt, fs::symlink};

#[cfg(target_os = "linux")]
#[test]
fn reserve_requires_a_private_root_and_rejects_linked_subdirectories() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    let root = data.join("Trash");
    fs::create_dir_all(root.join("files")).unwrap();
    fs::create_dir(root.join("info")).unwrap();
    let source = temp.path().join("source");
    fs::write(&source, b"preserved").unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(reserve(&data, &source).unwrap().is_none());
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    fs::remove_dir(root.join("files")).unwrap();
    let outside = temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    symlink(&outside, root.join("files")).unwrap();
    assert!(reserve(&data, &source).unwrap().is_none());
    assert!(fs::read_dir(outside).unwrap().next().is_none());
    assert!(fs::read_dir(root.join("info")).unwrap().next().is_none());
    assert_eq!(fs::read(source).unwrap(), b"preserved");
}

fn item(data: &Path, name: &str, original: &Path) -> PathBuf {
    let root = data.join("Trash");
    fs::create_dir_all(root.join("files")).unwrap();
    fs::create_dir_all(root.join("info")).unwrap();
    let encoded: String = original
        .as_os_str()
        .as_bytes()
        .iter()
        .map(|b| format!("%{b:02X}"))
        .collect();
    fs::write(
        root.join("info").join(format!("{name}.trashinfo")),
        format!("[Trash Info]\nPath={encoded}\nDeletionDate=2026-10-05T10:00:00\n"),
    )
    .unwrap();
    let source = root.join("files").join(name);
    fs::write(&source, "saved contents").unwrap();
    source
}

#[test]
fn list_restore_and_undo_from_persisted_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    let destination = temp.path().join("original name.txt");
    let source = item(&data, "original name.txt.2", &destination);
    let entries = read(&data).unwrap();
    assert!(
        entries
            .iter()
            .any(|entry| entry.path == source && entry.name == "original name.txt")
    );
    restore(&data, std::slice::from_ref(&source)).unwrap();
    assert_eq!(fs::read_to_string(&destination).unwrap(), "saved contents");
    assert!(!source.exists());
    assert!(
        !data
            .join("Trash/info/original name.txt.2.trashinfo")
            .exists()
    );
    assert!(super::super::undo::undo(&data).unwrap());
    assert!(!destination.exists());
    assert_eq!(fs::read_to_string(&source).unwrap(), "saved contents");
    assert!(
        data.join("Trash/info/original name.txt.2.trashinfo")
            .exists()
    );
}

#[test]
fn collisions_and_missing_parents_leave_trash_intact() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    let destination = temp.path().join("existing");
    let source = item(&data, "entry", &destination);
    symlink("missing", &destination).unwrap();
    assert_eq!(
        restore(&data, std::slice::from_ref(&source))
            .unwrap_err()
            .kind(),
        io::ErrorKind::AlreadyExists
    );
    assert!(source.exists());
    fs::remove_file(destination).unwrap();
    let absent = temp.path().join("absent/file");
    let other = item(&data, "other", &absent);
    assert!(restore(&data, std::slice::from_ref(&other)).is_err());
    assert!(other.exists());
}

#[test]
fn directory_symlink_and_batch_conflicts() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    let destination = temp.path().join("folder");
    let source = item(&data, "folder.1", &destination);
    fs::remove_file(&source).unwrap();
    fs::create_dir(&source).unwrap();
    symlink("missing", source.join("link")).unwrap();
    restore(&data, &[source]).unwrap();
    assert_eq!(
        fs::read_link(destination.join("link")).unwrap(),
        PathBuf::from("missing")
    );
    let target = temp.path().join("same");
    let a = item(&data, "a", &target);
    let b = item(&data, "b", &target);
    assert!(restore(&data, &[a.clone(), b.clone()]).is_err());
    assert!(a.exists() && b.exists());
    assert!(!target.exists());
}

#[cfg(target_os = "linux")]
#[test]
fn aliased_destinations_are_rejected_before_restoring_a_batch() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    let parent = temp.path().join("parent");
    let alias = temp.path().join("alias");
    fs::create_dir(&parent).unwrap();
    symlink(&parent, &alias).unwrap();
    let first = item(&data, "first", &parent.join("original"));
    let second = item(&data, "second", &alias.join("original"));
    assert_eq!(
        restore(&data, &[first.clone(), second.clone()])
            .unwrap_err()
            .kind(),
        io::ErrorKind::AlreadyExists
    );
    assert!(first.exists() && second.exists());
    assert!(!parent.join("original").exists());
    assert!(!super::super::undo::undo(&data).unwrap());
}

#[test]
fn rejects_invalid_metadata_and_non_trash_sources() {
    let temp = tempfile::tempdir().unwrap();
    let info = temp.path().join("bad.trashinfo");
    for value in ["../escape", "%ZZ", "%00", ""] {
        fs::write(&info, format!("[Trash Info]\nPath={value}")).unwrap();
        assert!(original(&info, Some(temp.path())).is_err());
    }
    assert!(restore(temp.path(), &[info]).is_err());
    assert_eq!(
        original(
            &{
                let p = temp.path().join("relative");
                fs::write(&p, "[Trash Info]\nPath=folder/file").unwrap();
                p
            },
            Some(temp.path())
        )
        .unwrap(),
        temp.path().join("folder/file")
    );
}

// APFS normalizes and rejects raw non-UTF-8 filename bytes.
#[cfg(target_os = "linux")]
#[test]
fn restores_a_selection_as_one_undoable_action_with_non_utf8_names() {
    use std::os::unix::ffi::OsStringExt;
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    let destination = temp
        .path()
        .join(std::ffi::OsString::from_vec(b"name\xff".to_vec()));
    let first = item(&data, "first", &destination);
    let second_destination = temp.path().join("second");
    let second = item(&data, "second.2", &second_destination);
    restore(&data, &[first.clone(), second.clone()]).unwrap();
    assert!(destination.exists() && second_destination.exists());
    assert!(super::super::undo::undo(&data).unwrap());
    assert!(first.exists() && second.exists());
    assert!(!destination.exists() && !second_destination.exists());
}

#[test]
fn empty_trash_removes_contents_and_orphans_without_following_links() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    let outside = temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("keep"), "preserved").unwrap();
    let source = item(&data, "folder", &outside);
    fs::remove_file(&source).unwrap();
    fs::create_dir(&source).unwrap();
    fs::write(source.join("nested"), "deleted").unwrap();
    symlink(&outside, source.join("link")).unwrap();
    let root = data.join("Trash");
    symlink(&outside, root.join("files/outside-link")).unwrap();
    fs::write(root.join("files/missing-info"), "deleted").unwrap();
    fs::write(root.join("info/orphan.trashinfo"), "invalid").unwrap();
    empty_root(&root).unwrap();
    assert_eq!(fs::read_dir(root.join("files")).unwrap().count(), 0);
    assert_eq!(fs::read_dir(root.join("info")).unwrap().count(), 0);
    assert_eq!(
        fs::read_to_string(outside.join("keep")).unwrap(),
        "preserved"
    );
    empty_root(&root).unwrap();
}

#[test]
fn empty_trash_rejects_a_symlinked_files_directory() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("Trash");
    let outside = temp.path().join("outside");
    fs::create_dir_all(root.join("info")).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("keep"), "preserved").unwrap();
    symlink(&outside, root.join("files")).unwrap();
    assert_eq!(
        empty_root(&root).unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
    assert!(outside.join("keep").exists());
}
