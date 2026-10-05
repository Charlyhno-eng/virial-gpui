use super::*;
use std::{
    ffi::OsString,
    os::unix::{ffi::OsStringExt, fs::symlink},
    process::Command,
};

fn create(data: &Path, directory: &Path, name: &str, folder: bool) {
    execute(
        data,
        Operation::New {
            directory: directory.into(),
            name: name.into(),
            folder,
        },
    )
    .unwrap();
}

#[test]
fn undoes_multiple_changes_using_only_persisted_history() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let files = root.path().join("files");
    fs::create_dir(&files).unwrap();
    create(&data, &files, "original.txt", false);
    execute(
        &data,
        Operation::Rename {
            source: files.join("original.txt"),
            name: "renamed.txt".into(),
        },
    )
    .unwrap();
    // No in-memory history or startup initialization is needed after reopening.
    let reopened_data = PathBuf::from(data.as_os_str());
    assert!(undo(&reopened_data).unwrap());
    assert!(files.join("original.txt").is_file());
    assert!(!files.join("renamed.txt").exists());
    assert!(undo(&reopened_data).unwrap());
    assert!(!files.join("original.txt").exists());
    assert!(!undo(&reopened_data).unwrap());
}

#[test]
fn history_survives_the_process_that_performed_the_action() {
    let root = tempfile::tempdir().unwrap();
    let result = Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "infrastructure::undo::tests::restart_worker",
        ])
        .env("VIRIAL_UNDO_RESTART_TEST_DIR", root.path())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(root.path().join("renamed").exists());
    assert!(undo(&root.path().join("data")).unwrap());
    assert!(root.path().join("original").exists());
    assert!(!root.path().join("renamed").exists());
    assert!(undo(&root.path().join("data")).unwrap());
    assert!(!root.path().join("original").exists());
}

#[test]
#[ignore = "run by history_survives_the_process_that_performed_the_action"]
fn restart_worker() {
    let root = PathBuf::from(std::env::var_os("VIRIAL_UNDO_RESTART_TEST_DIR").unwrap());
    let data = root.join("data");
    create(&data, &root, "original", false);
    execute(
        &data,
        Operation::Rename {
            source: root.join("original"),
            name: "renamed".into(),
        },
    )
    .unwrap();
}

#[test]
fn restores_modified_times_and_permissions_and_discards_read_only_backups() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let directory = root.path().join("folder");
    fs::create_dir(&directory).unwrap();
    let file = directory.join("file");
    fs::write(&file, "original").unwrap();
    let modified = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
    File::open(&file)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(modified))
        .unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o440)).unwrap();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o550)).unwrap();
    execute(
        &data,
        Operation::Rename {
            source: directory.clone(),
            name: "renamed".into(),
        },
    )
    .unwrap();
    assert!(undo(&data).unwrap());
    assert_eq!(fs::metadata(&file).unwrap().modified().unwrap(), modified);
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o440
    );
    assert_eq!(
        fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
        0o550
    );
    assert!(entries(&data.join("virial/undo")).unwrap().is_empty());
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o750)).unwrap();
}

#[test]
fn undoing_rename_and_move_preserves_external_hard_links() {
    use std::os::unix::fs::MetadataExt;
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let file = root.path().join("file");
    let link = root.path().join("hard-link");
    let destination = root.path().join("destination");
    fs::create_dir(&destination).unwrap();
    fs::write(&file, "original").unwrap();
    fs::hard_link(&file, &link).unwrap();
    execute(
        &data,
        Operation::Rename {
            source: file.clone(),
            name: "renamed".into(),
        },
    )
    .unwrap();
    assert!(undo(&data).unwrap());
    assert_eq!(
        fs::metadata(&file).unwrap().ino(),
        fs::metadata(&link).unwrap().ino()
    );
    execute(
        &data,
        Operation::Transfer {
            sources: vec![file.clone()],
            directory: destination,
            cut: true,
        },
    )
    .unwrap();
    assert!(undo(&data).unwrap());
    assert_eq!(
        fs::metadata(&file).unwrap().ino(),
        fs::metadata(&link).unwrap().ino()
    );
}

#[test]
fn undoes_folder_copy_and_batch_move_with_links_and_descendants() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let source = root.path().join("source");
    let target = root.path().join("target");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&target).unwrap();
    fs::create_dir(source.join("folder")).unwrap();
    fs::write(source.join("folder/file"), "original").unwrap();
    symlink("missing", source.join("link")).unwrap();
    for cut in [false, true] {
        execute(
            &data,
            Operation::Transfer {
                sources: vec![
                    source.join("folder"),
                    source.join("folder/file"),
                    source.join("link"),
                ],
                directory: target.clone(),
                cut,
            },
        )
        .unwrap();
        assert_eq!(fs::read(target.join("folder/file")).unwrap(), b"original");
        assert!(undo(&data).unwrap());
        assert_eq!(fs::read(source.join("folder/file")).unwrap(), b"original");
        assert_eq!(
            fs::read_link(source.join("link")).unwrap(),
            Path::new("missing")
        );
        assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
    }
}

#[test]
fn refuses_later_edits_and_name_collisions_without_losing_history() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let file = root.path().join("original");
    fs::write(&file, "original").unwrap();
    execute(
        &data,
        Operation::Rename {
            source: file.clone(),
            name: "renamed".into(),
        },
    )
    .unwrap();
    fs::write(root.path().join("renamed"), "edited").unwrap();
    assert!(undo(&data).is_err());
    assert_eq!(fs::read(root.path().join("renamed")).unwrap(), b"edited");
    fs::write(root.path().join("renamed"), "original").unwrap();
    symlink("missing", &file).unwrap();
    assert!(undo(&data).is_err());
    assert_eq!(fs::read_link(&file).unwrap(), Path::new("missing"));
    fs::remove_file(&file).unwrap();
    assert!(undo(&data).unwrap());
    assert_eq!(fs::read(file).unwrap(), b"original");
}

#[test]
fn refuses_to_remove_a_created_folder_with_new_contents() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    create(&data, root.path(), "folder", true);
    fs::write(root.path().join("folder/new"), "keep me").unwrap();
    assert!(undo(&data).is_err());
    assert_eq!(
        fs::read(root.path().join("folder/new")).unwrap(),
        b"keep me"
    );
    fs::remove_file(root.path().join("folder/new")).unwrap();
    assert!(undo(&data).unwrap());
}

#[test]
fn failures_and_noops_preserve_the_previous_action() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    create(&data, root.path(), "file", false);
    assert!(
        execute(
            &data,
            Operation::New {
                directory: root.path().into(),
                name: "file".into(),
                folder: false
            }
        )
        .is_err()
    );
    execute(
        &data,
        Operation::Transfer {
            sources: vec![root.path().join("file")],
            directory: root.path().into(),
            cut: true,
        },
    )
    .unwrap();
    assert!(undo(&data).unwrap());
    assert!(!root.path().join("file").exists());
    assert!(!undo(&data).unwrap());
}

#[test]
fn records_partial_success_and_checks_entire_batch_before_undo() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let a = root.path().join("a");
    let b = root.path().join("b");
    fs::write(&a, "before a").unwrap();
    fs::write(&b, "before b").unwrap();
    let result: io::Result<()> = record(&data, vec![a.clone(), b.clone()], || {
        fs::write(&a, "after a")?;
        fs::write(&b, "after b")?;
        Err(io::Error::other("simulated batch failure"))
    });
    assert!(result.is_err());
    fs::write(&b, "external edit").unwrap();
    assert!(undo(&data).is_err());
    assert_eq!(fs::read(&a).unwrap(), b"after a");
    fs::write(&b, "after b").unwrap();
    // A retry also accepts paths already restored by a previous partial undo.
    fs::write(&a, "before a").unwrap();
    assert!(undo(&data).unwrap());
    assert_eq!(fs::read(&b).unwrap(), b"before b");
}

#[test]
fn damaged_backup_blocks_undo_before_any_changes() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let file = root.path().join("original");
    fs::write(&file, "original").unwrap();
    execute(
        &data,
        Operation::Rename {
            source: file.clone(),
            name: "renamed".into(),
        },
    )
    .unwrap();
    let (_, directory) = entries(&data.join("virial/undo")).unwrap().pop().unwrap();
    fs::write(directory.join("0"), "damaged").unwrap();
    assert!(undo(&data).is_err());
    assert!(!file.exists());
    assert_eq!(fs::read(root.path().join("renamed")).unwrap(), b"original");
}

#[test]
fn preserves_non_utf8_paths_and_resolves_parent_aliases() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let source = root.path().join("source");
    fs::create_dir(&source).unwrap();
    symlink(&source, root.path().join("alias")).unwrap();
    let name = OsString::from_vec(vec![b'f', 0xff]);
    fs::write(source.join(&name), "bytes").unwrap();
    execute(
        &data,
        Operation::Rename {
            source: root.path().join("alias").join(&name),
            name: "renamed".into(),
        },
    )
    .unwrap();
    fs::remove_file(root.path().join("alias")).unwrap();
    assert!(undo(&data).unwrap());
    assert_eq!(fs::read(source.join(name)).unwrap(), b"bytes");
}

#[test]
fn bounds_history_and_does_not_record_launches() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    for index in 0..LIMIT + 2 {
        create(&data, root.path(), &format!("file{index}"), false);
    }
    assert_eq!(entries(&data.join("virial/undo")).unwrap().len(), LIMIT);
    // A failed external launch neither adds history nor hides the latest file action.
    assert!(
        execute(
            &data,
            Operation::Launch {
                desktop: root.path().join("missing.desktop"),
                file: root.path().join("file0")
            }
        )
        .is_err()
    );
    for _ in 0..LIMIT {
        assert!(undo(&data).unwrap());
    }
    assert!(!undo(&data).unwrap());
    assert!(root.path().join("file0").exists());
    assert!(root.path().join("file1").exists());
}

#[test]
fn refuses_actions_containing_the_history_and_unwritable_history() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let error = execute(
        &data,
        Operation::Rename {
            source: data.clone(),
            name: "other".into(),
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("undo history"));
    assert!(data.exists());
    let bad_data = root.path().join("bad-data");
    fs::write(&bad_data, "not a directory").unwrap();
    assert!(
        execute(
            &bad_data,
            Operation::New {
                directory: root.path().into(),
                name: "must-not-exist".into(),
                folder: false
            }
        )
        .is_err()
    );
    assert!(!root.path().join("must-not-exist").exists());
}

fn zip_fixture(path: &Path) {
    let mut writer = zip::ZipWriter::new(File::create(path).unwrap());
    writer.set_comment("preserved");
    writer
        .start_file("file.txt", zip::write::SimpleFileOptions::default())
        .unwrap();
    writer.write_all(b"original").unwrap();
    writer.finish().unwrap();
}

#[test]
fn undoes_zip_edits_and_transfers_between_local_files_and_archives() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let archive = root.path().join("sample.zip");
    let other = root.path().join("other.zip");
    zip_fixture(&archive);
    zip_fixture(&other);
    let original = fs::read(&archive).unwrap();
    let other_original = fs::read(&other).unwrap();
    execute(
        &data,
        Operation::Rename {
            source: archive.join("file.txt"),
            name: "renamed.txt".into(),
        },
    )
    .unwrap();
    create(&data, &archive, "folder", true);
    assert!(undo(&data).unwrap());
    assert!(undo(&data).unwrap());
    assert_eq!(fs::read(&archive).unwrap(), original);
    let local = root.path().join("local");
    fs::create_dir(&local).unwrap();
    execute(
        &data,
        Operation::Transfer {
            sources: vec![archive.join("file.txt")],
            directory: local.clone(),
            cut: true,
        },
    )
    .unwrap();
    assert_eq!(fs::read(local.join("file.txt")).unwrap(), b"original");
    assert!(undo(&data).unwrap());
    assert_eq!(fs::read(&archive).unwrap(), original);
    assert!(!local.join("file.txt").exists());
    fs::write(local.join("local.txt"), "local").unwrap();
    execute(
        &data,
        Operation::Transfer {
            sources: vec![local.join("local.txt")],
            directory: archive.clone(),
            cut: true,
        },
    )
    .unwrap();
    assert!(undo(&data).unwrap());
    assert_eq!(fs::read(&archive).unwrap(), original);
    assert_eq!(fs::read(local.join("local.txt")).unwrap(), b"local");
    create(&data, &other, "destination", true);
    let other_with_folder = fs::read(&other).unwrap();
    execute(
        &data,
        Operation::Transfer {
            sources: vec![archive.join("file.txt")],
            directory: other.join("destination"),
            cut: true,
        },
    )
    .unwrap();
    assert!(undo(&data).unwrap());
    assert_eq!(fs::read(&archive).unwrap(), original);
    assert_eq!(fs::read(&other).unwrap(), other_with_folder);
    assert!(undo(&data).unwrap());
    assert_eq!(fs::read(&other).unwrap(), other_original);
}

#[test]
fn undoes_compression_and_workspace_changes() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let file = root.path().join("file");
    fs::write(&file, "original").unwrap();
    execute(&data, Operation::Compress(file.clone())).unwrap();
    assert!(undo(&data).unwrap());
    assert!(!root.path().join("file.tar.gz").exists());
    assert_eq!(fs::read(file).unwrap(), b"original");
    let workspaces = data.join("virial/workspaces");
    record(&data, vec![workspaces.clone()], || {
        super::super::workspaces::edit(
            &data,
            super::super::workspaces::Edit::Add {
                name: "Project".into(),
                folder: None,
            },
        )
    })
    .unwrap();
    assert_eq!(super::super::workspaces::read(&data).unwrap().len(), 1);
    assert!(undo(&data).unwrap());
    assert!(!workspaces.exists());
}

#[test]
fn restores_desktop_trash_in_an_isolated_process() {
    let root = tempfile::tempdir().unwrap();
    let result = Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "infrastructure::undo::tests::trash_worker",
            "--nocapture",
        ])
        .env("VIRIAL_UNDO_TRASH_TEST_DIR", root.path())
        .env("HOME", root.path())
        .env("XDG_DATA_HOME", root.path().join("data"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
#[ignore = "run by restores_desktop_trash_in_an_isolated_process with isolated desktop paths"]
fn trash_worker() {
    let root = PathBuf::from(std::env::var_os("VIRIAL_UNDO_TRASH_TEST_DIR").unwrap());
    let data = root.join("data");
    let file = root.join("file");
    fs::write(&file, "restore me").unwrap();
    execute(&data, Operation::Trash(vec![file.clone()])).unwrap();
    assert!(!file.exists());
    assert!(undo(&data).unwrap());
    assert_eq!(fs::read(file).unwrap(), b"restore me");
}
