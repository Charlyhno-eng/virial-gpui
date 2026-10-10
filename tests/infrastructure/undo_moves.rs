use super::*;
use crate::infrastructure::{
    operations::Operation, progress::Progress, queue, trash as desktop_trash,
};
use std::{
    ffi::OsString,
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::symlink,
    },
    process::Command,
};

#[test]
fn restore_large_trash_items_without_snapshots_and_preserve_edits_during_undo() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let source = root.path().join("large-folder");
    fs::create_dir(&source).unwrap();
    let large = File::create(source.join("large")).unwrap();
    large.set_len(1024 * 1024 * 1024 * 1024).unwrap();
    let inode = large.metadata().unwrap().ino();
    let fifo = std::ffi::CString::new(source.join("fifo").as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    let link = root.path().join(OsString::from_vec(b"link\xff".to_vec()));
    symlink("missing", &link).unwrap();
    assert!(trash(&data, None, &[source.clone(), link.clone()], None).unwrap());
    let selected: Vec<_> = desktop_trash::read(&data)
        .unwrap()
        .into_iter()
        .map(|entry| entry.path)
        .collect();
    let info = data.join("Trash/info");
    let metadata: Vec<_> = fs::read_dir(&info)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            let contents = fs::read(&path).unwrap();
            (path, contents)
        })
        .collect();
    desktop_trash::restore(&data, &selected).unwrap();
    assert_eq!(fs::metadata(source.join("large")).unwrap().ino(), inode);
    assert_eq!(fs::read_link(&link).unwrap(), Path::new("missing"));
    assert_eq!(fs::read_dir(&info).unwrap().count(), 0);
    let journal = entries(&data.join("virial/undo")).unwrap().pop().unwrap().1;
    assert!(journal.join("moves.json").exists());
    assert!(!journal.join("0").exists());
    assert!(fs::metadata(journal.join("moves.json")).unwrap().len() < 4096);
    fs::write(source.join("edit"), b"edited after restoring").unwrap();
    assert!(super::super::undo(&data).unwrap());
    assert!(!source.exists() && !link.exists());
    for (path, contents) in metadata {
        assert_eq!(fs::read(path).unwrap(), contents);
    }
    let folder = desktop_trash::read(&data)
        .unwrap()
        .into_iter()
        .find(|entry| entry.directory)
        .unwrap();
    assert_eq!(
        fs::metadata(folder.path.join("large")).unwrap().ino(),
        inode
    );
    assert_eq!(
        fs::read(folder.path.join("edit")).unwrap(),
        b"edited after restoring"
    );
    // The previous Trash journal is still compatible with the restoration undo.
    assert!(super::super::undo(&data).unwrap());
    assert_eq!(
        fs::read(source.join("edit")).unwrap(),
        b"edited after restoring"
    );
}

#[test]
fn restore_undo_refuses_replaced_metadata_and_payloads_and_can_be_retried() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let source = root.path().join("original");
    fs::write(&source, b"original contents").unwrap();
    assert!(trash(&data, None, std::slice::from_ref(&source), None).unwrap());
    let trashed = desktop_trash::read(&data).unwrap().pop().unwrap().path;
    let mut name = trashed.file_name().unwrap().to_os_string();
    name.push(".trashinfo");
    let info = data.join("Trash/info").join(name);
    desktop_trash::restore(&data, std::slice::from_ref(&trashed)).unwrap();
    fs::write(&info, b"someone else's metadata").unwrap();
    assert!(super::super::undo(&data).is_err());
    assert_eq!(fs::read(&info).unwrap(), b"someone else's metadata");
    assert_eq!(fs::read(&source).unwrap(), b"original contents");
    fs::remove_file(&info).unwrap();
    let parked = root.path().join("parked");
    fs::rename(&source, &parked).unwrap();
    fs::write(&source, b"replacement contents").unwrap();
    assert!(super::super::undo(&data).is_err());
    assert_eq!(fs::read(&source).unwrap(), b"replacement contents");
    assert!(!info.exists());
    fs::remove_file(&source).unwrap();
    fs::rename(parked, &source).unwrap();
    assert!(super::super::undo(&data).unwrap());
    assert_eq!(fs::read(trashed).unwrap(), b"original contents");
    assert!(info.exists());
}

#[test]
fn interrupted_restores_keep_complete_metadata_and_remain_undoable() {
    for phase in 0..5 {
        for resume in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let data = root.path().join("data");
            fs::create_dir(&data).unwrap();
            let mut planned = Vec::new();
            for name in ["first", "second"] {
                let destination = root.path().join(name);
                fs::write(&destination, name).unwrap();
                let (source, info) = desktop_trash::reserve(&data, &destination)
                    .unwrap()
                    .unwrap();
                operations::rename(&destination, &source).unwrap();
                let info = info.keep().unwrap().1;
                let mut item = Move::new(&source, &destination, None).unwrap();
                item.restored_info = Some(TrashInfo {
                    location: uri(&info).unwrap(),
                    contents: fs::read(info).unwrap(),
                });
                planned.push(item);
            }
            let directory = {
                let _guard = LOCK.lock().unwrap();
                let (history, _lock) = history(&data).unwrap();
                publish(&history, &[], &planned, None, None).unwrap()
            };
            if phase > 0 {
                operations::rename(
                    &path(&planned[0].source).unwrap(),
                    &path(&planned[0].destination).unwrap(),
                )
                .unwrap();
            }
            if phase > 1 {
                planned[0].restored_info.as_ref().unwrap().remove().unwrap();
            }
            if phase > 2 {
                operations::rename(
                    &path(&planned[1].source).unwrap(),
                    &path(&planned[1].destination).unwrap(),
                )
                .unwrap();
                planned[1].restored_info.as_ref().unwrap().remove().unwrap();
            }
            if phase > 3 {
                // Interruption during undo after publishing metadata, before moving payload.
                planned[1]
                    .restored_info
                    .as_ref()
                    .unwrap()
                    .restore()
                    .unwrap();
            }
            if resume {
                apply(&load(&directory).unwrap().unwrap(), None).unwrap();
            }
            assert!(super::super::undo(&data).unwrap());
            for item in planned {
                assert!(path(&item.source).unwrap().exists());
                assert!(!path(&item.destination).unwrap().exists());
                let info = item.restored_info.unwrap();
                assert_eq!(
                    fs::read(path(&info.location).unwrap()).unwrap(),
                    info.contents
                );
            }
        }
    }
}

#[test]
fn queued_trash_with_desktop_permissions_never_snapshots_large_contents() {
    use std::os::unix::fs::PermissionsExt;
    for mode in [0o755, 0o775] {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        let trash_root = data.join("Trash");
        fs::create_dir_all(trash_root.join("files")).unwrap();
        fs::create_dir(trash_root.join("info")).unwrap();
        fs::set_permissions(&trash_root, fs::Permissions::from_mode(0o700)).unwrap();
        for name in ["files", "info"] {
            fs::set_permissions(trash_root.join(name), fs::Permissions::from_mode(mode)).unwrap();
        }
        let source = root.path().join("large-folder");
        fs::create_dir(&source).unwrap();
        let large = File::create(source.join("large")).unwrap();
        large.set_len(1024 * 1024 * 1024 * 1024).unwrap();
        let inode = large.metadata().unwrap().ino();
        // The former snapshot fallback cannot handle a FIFO; a native move can.
        let fifo = std::ffi::CString::new(source.join("fifo").as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        let job = queue::enqueue(&data, &Operation::Trash(vec![source.clone()]), true).unwrap();
        queue::execute(&data, job, &Progress::default()).unwrap();
        assert!(!source.exists());
        assert!(queue::recover(&data).unwrap().is_empty());
        let trashed = desktop_trash::read(&data).unwrap().pop().unwrap();
        assert_eq!(
            fs::metadata(trashed.path.join("large")).unwrap().ino(),
            inode
        );
        let entry = entries(&data.join("virial/undo")).unwrap().pop().unwrap().1;
        assert!(entry.join("moves.json").exists());
        assert!(!entry.join("0").exists());
        assert!(super::super::undo(&data).unwrap());
        assert_eq!(fs::metadata(source.join("large")).unwrap().ino(), inode);
        for name in ["files", "info"] {
            assert_eq!(
                fs::metadata(trash_root.join(name)).unwrap().mode() & 0o777,
                mode
            );
        }
    }
}

#[test]
fn renames_large_sparse_trees_without_reading_or_copying_their_contents() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let folder = root.path().join("folder");
    fs::create_dir(&folder).unwrap();
    let large = File::create(folder.join("large")).unwrap();
    large.set_len(1024 * 1024 * 1024 * 1024).unwrap();
    let inode = large.metadata().unwrap().ino();
    symlink("missing", folder.join("link")).unwrap();
    // A FIFO makes a content/snapshot-based implementation fail or block.
    let fifo = std::ffi::CString::new(folder.join("fifo").as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    rename(&data, &folder, "renamed", Some(&Progress::default())).unwrap();
    let directory = entries(&data.join("virial/undo")).unwrap().pop().unwrap().1;
    assert!(!directory.join("0").exists());
    assert!(fs::metadata(directory.join("moves.json")).unwrap().len() < 2048);
    assert_eq!(
        fs::metadata(root.path().join("renamed/large"))
            .unwrap()
            .ino(),
        inode
    );
    assert!(super::super::undo(&data).unwrap());
    assert_eq!(fs::metadata(folder.join("large")).unwrap().ino(), inode);
    assert_eq!(
        fs::read_link(folder.join("link")).unwrap(),
        Path::new("missing")
    );
}

#[test]
fn refuses_replaced_items_and_preserves_the_journal_for_retry() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let source = root.path().join("source");
    let destination = root.path().join("renamed");
    fs::write(&source, b"original").unwrap();
    rename(&data, &source, "renamed", None).unwrap();
    fs::rename(&destination, root.path().join("retained")).unwrap();
    fs::write(&destination, b"replacement").unwrap();
    assert!(super::super::undo(&data).is_err());
    assert_eq!(fs::read(&destination).unwrap(), b"replacement");
    assert!(!source.exists());
    fs::remove_file(&destination).unwrap();
    fs::rename(root.path().join("retained"), &destination).unwrap();
    assert!(super::super::undo(&data).unwrap());
    assert_eq!(fs::read(&source).unwrap(), b"original");
}

#[test]
fn trashes_and_restores_non_utf8_names_links_and_selected_descendants() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let folder = root.path().join("folder #\n%");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("child"), b"original").unwrap();
    let link = root.path().join(OsString::from_vec(vec![b'l', 0xff]));
    symlink("missing", &link).unwrap();
    let sources = vec![
        folder.clone(),
        folder.join("child"),
        link.clone(),
        folder.clone(),
    ];
    assert!(trash(&data, None, &sources, Some(&Progress::default())).unwrap());
    let entries = desktop_trash::read(&data).unwrap();
    assert_eq!(entries.len(), 2);
    let trashed_folder = entries.iter().find(|entry| entry.directory).unwrap();
    fs::write(trashed_folder.path.join("child"), b"later edit").unwrap();
    assert!(super::super::undo(&data).unwrap());
    assert_eq!(fs::read(folder.join("child")).unwrap(), b"later edit");
    assert_eq!(fs::read_link(&link).unwrap(), Path::new("missing"));
    assert_eq!(fs::read_dir(data.join("Trash/info")).unwrap().count(), 0);
    assert!(desktop_trash::read(&data).unwrap().is_empty());
}

fn planned_batch(root: &Path, moved: bool) -> (PathBuf, queue::Job) {
    let data = root.join("data");
    let sources = [root.join("one"), root.join("two")];
    for source in &sources {
        fs::write(source, source.file_name().unwrap().as_bytes()).unwrap();
    }
    let job = queue::enqueue(&data, &Operation::Trash(sources.to_vec()), false).unwrap();
    let _guard = LOCK.lock().unwrap();
    let (history, _lock) = history(&data).unwrap();
    let mut moves = Vec::new();
    let mut reservations = Vec::new();
    for source in &sources {
        let (destination, info) = desktop_trash::reserve(&data, source).unwrap().unwrap();
        moves.push(Move::new(source, &destination, Some(info.path())).unwrap());
        reservations.push(info);
    }
    publish(
        &history,
        &entries(&history).unwrap(),
        &moves,
        Some(&job.id),
        Some("Trash\t2"),
    )
    .unwrap();
    for info in reservations {
        info.keep().unwrap();
    }
    if moved {
        operations::rename(&sources[0], &path(&moves[0].destination).unwrap()).unwrap();
    }
    (data, job)
}

#[test]
fn resumes_partial_trash_after_a_process_exit_without_replaying_or_copying() {
    let root = tempfile::tempdir().unwrap();
    let result = Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "infrastructure::undo::moves::tests::interruption_worker",
        ])
        .env("VIRIAL_FAST_TRASH_TEST_ROOT", root.path())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let data = root.path().join("data");
    assert!(!root.path().join("one").exists());
    assert!(root.path().join("two").exists());
    let jobs = queue::recover(&data).unwrap();
    assert_eq!(jobs.len(), 1);
    queue::execute(&data, jobs[0].clone(), &Progress::default()).unwrap();
    assert!(queue::recover(&data).unwrap().is_empty());
    assert_eq!(entries(&data.join("virial/undo")).unwrap().len(), 1);
    assert_eq!(desktop_trash::read(&data).unwrap().len(), 2);
    assert!(super::super::undo(&data).unwrap());
    assert_eq!(fs::read(root.path().join("one")).unwrap(), b"one");
    assert_eq!(fs::read(root.path().join("two")).unwrap(), b"two");
}

#[test]
#[ignore = "run by resumes_partial_trash_after_a_process_exit_without_replaying_or_copying"]
fn interruption_worker() {
    let root = PathBuf::from(std::env::var_os("VIRIAL_FAST_TRASH_TEST_ROOT").unwrap());
    planned_batch(&root, true);
    std::process::exit(0);
}

#[test]
fn cancellation_keeps_partial_trash_undoable_and_cleans_reserved_metadata() {
    let root = tempfile::tempdir().unwrap();
    let (data, job) = planned_batch(root.path(), true);
    let progress = Progress::default();
    progress.cancel();
    let error = queue::execute(&data, job, &progress).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert!(queue::recover(&data).unwrap().is_empty());
    assert!(super::super::undo(&data).unwrap());
    assert_eq!(fs::read(root.path().join("one")).unwrap(), b"one");
    assert_eq!(fs::read(root.path().join("two")).unwrap(), b"two");
    assert_eq!(fs::read_dir(data.join("Trash/info")).unwrap().count(), 0);
}

#[test]
fn empty_trash_blocks_undo_without_recreating_or_overwriting_items() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let source = root.path().join("source");
    fs::write(&source, b"original").unwrap();
    assert!(trash(&data, None, std::slice::from_ref(&source), None).unwrap());
    desktop_trash::empty(&data).unwrap();
    assert!(super::super::undo(&data).is_err());
    assert!(!source.exists());
}

#[test]
fn missing_metadata_during_retry_leaves_remaining_sources_intact() {
    let root = tempfile::tempdir().unwrap();
    let (data, job) = planned_batch(root.path(), true);
    let directory = entries(&data.join("virial/undo")).unwrap().pop().unwrap().1;
    let moves = load(&directory).unwrap().unwrap();
    fs::remove_file(path(moves[1].info.as_ref().unwrap()).unwrap()).unwrap();
    assert!(queue::execute(&data, job.clone(), &Progress::default()).is_err());
    assert_eq!(fs::read(root.path().join("two")).unwrap(), b"two");
    assert_eq!(queue::recover(&data).unwrap().len(), 1);
    let cancelled = Progress::default();
    cancelled.cancel();
    assert_eq!(
        queue::execute(&data, job, &cancelled).unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    assert!(queue::recover(&data).unwrap().is_empty());
    assert!(super::super::undo(&data).unwrap());
    assert_eq!(fs::read(root.path().join("one")).unwrap(), b"one");
}

#[test]
fn checks_entire_trash_batch_before_restoring_any_item() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let sources = [root.path().join("one"), root.path().join("two")];
    for source in &sources {
        fs::write(source, b"original").unwrap();
    }
    assert!(trash(&data, None, &sources, None).unwrap());
    fs::write(&sources[0], b"collision").unwrap();
    assert!(super::super::undo(&data).is_err());
    assert_eq!(desktop_trash::read(&data).unwrap().len(), 2);
    assert!(!sources[1].exists());
    fs::remove_file(&sources[0]).unwrap();
    assert!(super::super::undo(&data).unwrap());
    assert_eq!(fs::read(&sources[0]).unwrap(), b"original");
    assert_eq!(fs::read(&sources[1]).unwrap(), b"original");
}

#[test]
fn cancelling_before_any_move_preserves_all_previous_undo_entries() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    for index in 0..LIMIT {
        super::super::execute(
            &data,
            Operation::New {
                directory: root.path().into(),
                name: format!("created-{index}"),
                folder: false,
            },
        )
        .unwrap();
    }
    let (_, job) = planned_batch(root.path(), false);
    let progress = Progress::default();
    progress.cancel();
    assert_eq!(
        queue::execute(&data, job, &progress).unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    assert!(queue::recover(&data).unwrap().is_empty());
    assert_eq!(entries(&data.join("virial/undo")).unwrap().len(), LIMIT);
    assert_eq!(fs::read_dir(data.join("Trash/info")).unwrap().count(), 0);
    assert!(super::super::undo(&data).unwrap());
    assert!(!root.path().join(format!("created-{}", LIMIT - 1)).exists());
    assert!(root.path().join("one").exists());
    assert!(root.path().join("two").exists());
}

#[test]
fn cross_mount_fallback_restores_partial_moves_before_snapshotting() {
    let root = tempfile::tempdir().unwrap();
    let (data, job) = planned_batch(root.path(), true);
    let _guard = LOCK.lock().unwrap();
    let (history, _lock) = history(&data).unwrap();
    let directory = entries(&history).unwrap().pop().unwrap().1;
    let moves = load(&directory).unwrap().unwrap();
    assert!(
        !finish_trash(
            &history,
            &directory,
            &moves,
            Err(io::Error::from_raw_os_error(libc::EXDEV))
        )
        .unwrap()
    );
    assert_eq!(fs::read(root.path().join("one")).unwrap(), b"one");
    assert_eq!(fs::read(root.path().join("two")).unwrap(), b"two");
    assert!(entries(&history).unwrap().is_empty());
    assert_eq!(fs::read_dir(data.join("Trash/info")).unwrap().count(), 0);
    assert_eq!(queue::recover(&data).unwrap()[0].id, job.id);
}
