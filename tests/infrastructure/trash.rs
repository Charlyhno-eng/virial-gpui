use super::*;
use std::os::unix::{ffi::OsStrExt, fs::symlink};

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
    assert!(restore(&data, &[&a, &b]).is_err());
    assert!(a.exists() && b.exists());
    assert!(!target.exists());
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
    restore(&data, &[&first, &second]).unwrap();
    assert!(destination.exists() && second_destination.exists());
    assert!(super::super::undo::undo(&data).unwrap());
    assert!(first.exists() && second.exists());
    assert!(!destination.exists() && !second_destination.exists());
}
