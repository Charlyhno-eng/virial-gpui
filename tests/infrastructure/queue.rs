use super::*;
#[cfg(unix)]
use std::os::unix::{ffi::OsStringExt, fs::symlink};
#[cfg(windows)]
use std::os::windows::fs::{symlink_dir as symlink_dir_win, symlink_file as symlink_file_win};
use std::{ffi::OsString, process::Command, sync::mpsc, time::Duration};
#[cfg(windows)]
fn symlink<P: AsRef<std::path::Path>, Q: AsRef<std::path::Path>>(
    original: P,
    link: Q,
) -> std::io::Result<()> {
    if std::fs::metadata(&original)
        .map(|m| m.is_dir())
        .unwrap_or(false)
    {
        symlink_dir_win(original, link)
    } else {
        symlink_file_win(original, link)
    }
}

fn transfer_job(data: &Path, sources: Vec<PathBuf>, directory: &Path, cut: bool) -> Job {
    enqueue(
        data,
        &Operation::Transfer {
            sources,
            directory: directory.into(),
            cut,
        },
        true,
    )
    .unwrap()
}

fn wait_conflict(progress: &Progress) -> Conflict {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(conflict) = progress.snapshot().conflict {
            return conflict;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Conflict was not reported"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn pause_resume_and_cancel_interrupt_blocked_workers() {
    let progress = Progress::default();
    progress.toggle_pause();
    let (tx, rx) = mpsc::channel();
    let worker_progress = progress.clone();
    let worker = std::thread::spawn(move || {
        tx.send(worker_progress.checkpoint()).unwrap();
    });
    assert!(rx.recv_timeout(Duration::from_millis(30)).is_err());
    progress.toggle_pause();
    assert!(rx.recv_timeout(Duration::from_secs(1)).unwrap().is_ok());
    worker.join().unwrap();
    progress.toggle_pause();
    progress.cancel();
    assert_eq!(
        progress.checkpoint().unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    progress.begin(Phase::Finishing, None);
    assert!(progress.checkpoint().is_ok());
}

#[test]
fn persists_enqueue_order_and_cancels_waiting_jobs() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let target = root.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::write(root.path().join("a"), b"a").unwrap();
    fs::write(root.path().join("b"), b"b").unwrap();
    let first = transfer_job(&data, vec![root.path().join("a")], &target, false);
    let second = transfer_job(&data, vec![root.path().join("b")], &target, false);
    // Planning updates a journal's mtime without changing its queue position.
    save(&data, &first).unwrap();
    let jobs = recover(&data).unwrap();
    assert_eq!(
        jobs.iter().map(|job| &job.id).collect::<Vec<_>>(),
        vec![&first.id, &second.id]
    );
    forget(&data, &first).unwrap();
    assert_eq!(recover(&data).unwrap()[0].id, second.id);
    assert!(!target.join("a").exists());
}

#[test]
fn same_folder_move_skips_reading_the_unchanged_tree() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let data = root.path().join("data");
    fs::create_dir(&source).unwrap();
    let fifo = source.join("fifo");
    let name = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    let job = transfer_job(&data, vec![source.clone()], root.path(), true);
    execute(&data, job, &Progress::default()).unwrap();
    assert!(source.is_dir());
    assert!(fs::symlink_metadata(fifo).is_ok());
    assert!(recover(&data).unwrap().is_empty());
}

#[test]
fn self_transfer_is_rejected_before_reading_the_tree() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let target = source.join("target");
    fs::create_dir_all(&target).unwrap();
    let fifo = source.join("fifo");
    let name = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    let data = root.path().join("data");
    let job = transfer_job(&data, vec![source.clone()], &target, true);
    let error = execute(&data, job, &Progress::default()).unwrap_err();
    assert!(error.to_string().contains("itself"));
    assert!(source.is_dir());
    assert!(!target.join("source").exists());
}

#[test]
fn counting_tree_honors_pause_and_cancellation() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("folder")).unwrap();
    fs::write(root.path().join("folder/file"), b"contents").unwrap();
    symlink("folder", root.path().join("link")).unwrap();
    assert_eq!(counts_controlled(root.path(), None).unwrap(), (4, 8));
    let progress = Progress::default();
    progress.toggle_pause();
    let worker_progress = progress.clone();
    let source = root.path().to_path_buf();
    let (tx, rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        tx.send(counts_controlled(&source, Some(&worker_progress)))
            .unwrap();
    });
    assert!(rx.recv_timeout(Duration::from_millis(30)).is_err());
    progress.cancel();
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
    worker.join().unwrap();
}

// APFS normalizes and rejects raw non-UTF-8 filename bytes, so this fixture is
// only meaningfully exercisable on Linux.
#[cfg(target_os = "linux")]
#[test]
fn copies_tree_links_empty_and_non_utf8_names_as_one_undo_batch() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let source = root.path().join("source");
    let target = root.path().join("target");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&target).unwrap();
    let name = OsString::from_vec(vec![b'n', 0xff]);
    fs::write(source.join(&name), b"contents").unwrap();
    fs::write(source.join("empty"), b"").unwrap();
    symlink("missing", source.join("link")).unwrap();
    fs::set_permissions(source.join(&name), fs::Permissions::from_mode(0o440)).unwrap();
    let job = transfer_job(
        &data,
        vec![source.clone(), source.join(&name)],
        &target,
        false,
    );
    let progress = Progress::default();
    execute(&data, job, &progress).unwrap();
    assert_eq!(
        fs::read(target.join("source").join(&name)).unwrap(),
        b"contents"
    );
    assert_eq!(
        fs::read_link(target.join("source/link")).unwrap(),
        Path::new("missing")
    );
    assert!(!target.join(&name).exists());
    assert_eq!(progress.snapshot().files_done, 4);
    assert_eq!(progress.snapshot().bytes_done, 8);
    assert_eq!(
        undo::fingerprint(&source).unwrap(),
        undo::fingerprint(&target.join("source")).unwrap()
    );
    assert!(recover(&data).unwrap().is_empty());
    assert!(undo::undo(&data).unwrap());
    assert!(!target.join("source").exists());
    assert!(source.join(&name).exists());
    assert!(!undo::undo(&data).unwrap());
}

#[test]
fn duplicate_conflicts_can_skip_all_without_removing_move_sources() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let target = root.path().join("target");
    fs::create_dir(&target).unwrap();
    let sources = ["a", "b"].map(|name| {
        fs::write(root.path().join(name), b"same").unwrap();
        fs::write(target.join(name), b"same").unwrap();
        root.path().join(name)
    });
    let job = transfer_job(&data, sources.to_vec(), &target, true);
    let progress = Progress::default();
    std::thread::scope(|scope| {
        let task = scope.spawn(|| execute(&data, job, &progress));
        assert!(wait_conflict(&progress).identical);
        progress.resolve(Resolution::Skip, true);
        task.join().unwrap().unwrap();
    });
    assert!(sources.iter().all(|source| source.exists()));
    assert_eq!(fs::read(target.join("a")).unwrap(), b"same");
    assert!(!undo::undo(&data).unwrap());
}

#[test]
fn keep_both_handles_existing_and_colliding_selected_names_and_undo() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let target = root.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("notes.txt"), b"existing").unwrap();
    fs::write(target.join("notes (1).txt"), b"reserved").unwrap();
    let mut sources = Vec::new();
    for (index, contents) in [b"one".as_slice(), b"two".as_slice()]
        .into_iter()
        .enumerate()
    {
        let directory = root.path().join(index.to_string());
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("notes.txt"), contents).unwrap();
        sources.push(directory.join("notes.txt"));
    }
    let job = transfer_job(&data, sources.clone(), &target, true);
    let progress = Progress::default();
    std::thread::scope(|scope| {
        let task = scope.spawn(|| execute(&data, job, &progress));
        let conflict = wait_conflict(&progress);
        assert!(!conflict.identical);
        assert_eq!(conflict.destination_bytes, 8);
        progress.resolve(Resolution::KeepBoth, true);
        task.join().unwrap().unwrap();
    });
    assert_eq!(fs::read(target.join("notes.txt")).unwrap(), b"existing");
    assert_eq!(fs::read(target.join("notes (1).txt")).unwrap(), b"reserved");
    assert_eq!(fs::read(target.join("notes (2).txt")).unwrap(), b"one");
    assert_eq!(fs::read(target.join("notes (3).txt")).unwrap(), b"two");
    assert!(undo::undo(&data).unwrap());
    assert!(sources.iter().all(|source| source.exists()));
    assert!(!target.join("notes (2).txt").exists());
    assert!(!target.join("notes (3).txt").exists());
}

#[test]
fn cancel_while_resolving_conflicts_removes_journal_without_mutation() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let target = root.path().join("target");
    fs::create_dir(&target).unwrap();
    let source = root.path().join("file");
    fs::write(&source, b"source").unwrap();
    fs::write(target.join("file"), b"destination").unwrap();
    let job = transfer_job(&data, vec![source.clone()], &target, false);
    let progress = Progress::default();
    std::thread::scope(|scope| {
        let task = scope.spawn(|| execute(&data, job, &progress));
        wait_conflict(&progress);
        progress.cancel();
        assert_eq!(
            task.join().unwrap().unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
    });
    assert!(recover(&data).unwrap().is_empty());
    assert_eq!(fs::read(&source).unwrap(), b"source");
    assert_eq!(fs::read(target.join("file")).unwrap(), b"destination");
}

fn crash_fixture(mode: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("target")).unwrap();
    fs::write(root.path().join("first"), vec![42; 5 * 1024 * 1024 + 7]).unwrap();
    fs::write(root.path().join("second"), b"second").unwrap();
    let result = Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "infrastructure::queue::tests::crash_worker",
        ])
        .env("VIRIAL_QUEUE_TEST_ROOT", root.path())
        .env("VIRIAL_QUEUE_TEST_MODE", mode)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    root
}

#[test]
fn resumes_a_partial_large_file_after_process_exit_and_undoes_entire_job() {
    let root = crash_fixture("partial");
    let data = root.path().join("data");
    let jobs = recover(&data).unwrap();
    assert_eq!(jobs.len(), 1);
    execute(&data, jobs[0].clone(), &Progress::default()).unwrap();
    assert_eq!(
        fs::read(root.path().join("target/first")).unwrap(),
        fs::read(root.path().join("first")).unwrap()
    );
    assert_eq!(
        fs::read(root.path().join("target/second")).unwrap(),
        b"second"
    );
    assert!(undo::undo(&data).unwrap());
    assert!(!root.path().join("target/first").exists());
    assert!(!root.path().join("target/second").exists());
    assert!(!undo::undo(&data).unwrap());
}

#[test]
fn resumes_a_published_move_without_replaying_or_losing_originals() {
    let root = crash_fixture("published-move");
    let data = root.path().join("data");
    assert!(!root.path().join("first").exists());
    let job = recover(&data).unwrap().pop().unwrap();
    execute(&data, job, &Progress::default()).unwrap();
    assert!(!root.path().join("second").exists());
    assert!(root.path().join("target/first").exists());
    assert!(undo::undo(&data).unwrap());
    assert!(root.path().join("first").exists());
    assert!(root.path().join("second").exists());
}

#[test]
fn recovery_rejects_edited_destination_and_preserves_backups_and_journal() {
    let root = crash_fixture("published-move");
    let data = root.path().join("data");
    fs::write(root.path().join("target/first"), b"later edit").unwrap();
    let job = recover(&data).unwrap().pop().unwrap();
    let error = execute(&data, job, &Progress::default()).unwrap_err();
    assert!(error.to_string().contains("changed after interruption"));
    assert_eq!(recover(&data).unwrap().len(), 1);
    assert_eq!(
        fs::read(root.path().join("target/first")).unwrap(),
        b"later edit"
    );
    assert!(root.path().join("second").exists());
    assert!(undo::undo(&data).is_err());
}

#[test]
fn recovery_checks_saved_file_prefix_even_when_optional_verification_is_off() {
    let root = crash_fixture("partial");
    let data = root.path().join("data");
    let mut job = recover(&data).unwrap().pop().unwrap();
    job.verify = false;
    let staged = path(job.plan.as_ref().unwrap()[0].staging.as_ref().unwrap())
        .unwrap()
        .join("contents");
    fs::write(&staged, vec![99; 1024 * 1024]).unwrap();
    let error = execute(&data, job, &Progress::default()).unwrap_err();
    assert!(error.to_string().contains("integrity check"));
    assert!(!root.path().join("target/first").exists());
    assert!(root.path().join("first").exists());
    assert_eq!(recover(&data).unwrap().len(), 1);
    let progress = Progress::default();
    progress.cancel();
    let job = recover(&data).unwrap().pop().unwrap();
    assert_eq!(
        execute(&data, job, &progress).unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    assert!(recover(&data).unwrap().is_empty());
    assert!(!undo::undo(&data).unwrap());
}

#[test]
fn finalized_job_is_not_replayed_when_crash_precedes_journal_removal() {
    let root = crash_fixture("finalized");
    let data = root.path().join("data");
    let job = recover(&data).unwrap().pop().unwrap();
    // An edit after the completed action must not be replaced during recovery.
    fs::write(root.path().join("target/first"), b"later edit").unwrap();
    execute(&data, job, &Progress::default()).unwrap();
    assert_eq!(
        fs::read(root.path().join("target/first")).unwrap(),
        b"later edit"
    );
    assert!(recover(&data).unwrap().is_empty());
    assert!(undo::undo(&data).is_err());
}

#[test]
fn rejects_self_transfers_and_journal_folders() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let folder = root.path().join("folder");
    fs::create_dir(&folder).unwrap();
    let job = transfer_job(&data, vec![folder.clone()], &folder, false);
    assert!(
        execute(&data, job, &Progress::default())
            .unwrap_err()
            .to_string()
            .contains("itself")
    );
    let job = transfer_job(&data, vec![data.clone()], &folder, false);
    assert!(
        execute(&data, job, &Progress::default())
            .unwrap_err()
            .to_string()
            .contains("journals")
    );
}

#[test]
#[ignore = "isolated process used by crash recovery tests"]
fn crash_worker() {
    let root = PathBuf::from(std::env::var_os("VIRIAL_QUEUE_TEST_ROOT").unwrap());
    let mode = std::env::var("VIRIAL_QUEUE_TEST_MODE").unwrap();
    let data = root.join("data");
    let cut = mode == "published-move" || mode == "parked-move";
    if mode == "readonly" {
        fs::set_permissions(root.join("first"), fs::Permissions::from_mode(0o440)).unwrap();
    }
    let mut job = transfer_job(
        &data,
        vec![root.join("first"), root.join("second")],
        &root.join("target"),
        cut,
    );
    let progress = Progress::default();
    let plan = prepare(&data, &job, &progress).unwrap();
    job.plan = Some(plan.clone());
    save(&data, &job).unwrap();
    let mut expected = Vec::new();
    for item in &plan {
        if cut {
            expected.push((path(&item.source).unwrap(), "-".into()));
        }
        expected.push((
            path(item.destination.as_ref().unwrap()).unwrap(),
            item.hash.clone(),
        ));
    }
    undo::durable(&data, &job.id, expected, "Transfer\t2", &progress, || {
        progress.begin(Phase::Copying, None);
        if mode == "partial" {
            let item = &plan[0];
            let source = path(&item.source).unwrap();
            let staged = path(item.staging.as_ref().unwrap())
                .unwrap()
                .join("contents");
            let mut input = File::open(source).unwrap();
            let mut output = File::create(&staged).unwrap();
            io::copy(&mut (&mut input).take(1024 * 1024), &mut output).unwrap();
            output.sync_all().unwrap();
            std::process::exit(0);
        }
        if mode == "readonly" || mode == "parked-move" {
            let item = &plan[0];
            let source = path(&item.source).unwrap();
            let staged = path(item.staging.as_ref().unwrap())
                .unwrap()
                .join("contents");
            stage_copy(&source, &staged, &progress).unwrap();
            if mode == "parked-move" {
                operations::rename(&staged, &path(item.destination.as_ref().unwrap()).unwrap())
                    .unwrap();
                let parked = path(item.parked.as_ref().unwrap())
                    .unwrap()
                    .join("contents");
                operations::rename(&source, &parked).unwrap();
                sync_parent(&source).unwrap();
                sync_parent(&parked).unwrap();
            }
            std::process::exit(0);
        }
        transfer(&plan[0], cut, true, &progress).unwrap();
        if mode == "published-move" {
            std::process::exit(0);
        }
        transfer(&plan[1], cut, true, &progress).unwrap();
        Ok(())
    })
    .unwrap();
    std::process::exit(0);
}

#[test]
fn durable_trash_has_a_persisted_batch_summary_and_restores_every_item() {
    let root = tempfile::tempdir().unwrap();
    let result = Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "infrastructure::queue::tests::trash_worker",
        ])
        .env("VIRIAL_QUEUE_TEST_ROOT", root.path())
        .env("HOME", root.path().join("home"))
        .env("XDG_DATA_HOME", root.path().join("desktop-data"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
#[ignore = "isolated desktop Trash paths"]
fn trash_worker() {
    let root = PathBuf::from(std::env::var_os("VIRIAL_QUEUE_TEST_ROOT").unwrap());
    let data = root.join("data");
    fs::create_dir_all(root.join("home")).unwrap();
    let folder = root.join("deleted");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("one"), b"one").unwrap();
    fs::write(folder.join("two"), b"two").unwrap();
    let job = enqueue(&data, &Operation::Trash(vec![folder.clone()]), false).unwrap();
    execute(&data, job, &Progress::default()).unwrap();
    assert!(!folder.exists());
    assert_eq!(
        undo::latest_summary(&data)
            .unwrap()
            .map(|(_, summary)| summary),
        Some("Trash\t3".into())
    );
    assert!(undo::undo(&data).unwrap());
    assert_eq!(fs::read(folder.join("one")).unwrap(), b"one");
    assert_eq!(fs::read(folder.join("two")).unwrap(), b"two");
}

#[test]
fn bounded_parallel_transfers_share_progress_and_remain_one_undo_batch() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let target = root.path().join("target");
    fs::create_dir(&target).unwrap();
    let sources = (0..64)
        .map(|index| {
            let source = root.path().join(format!("file-{index}"));
            fs::write(&source, vec![index as u8; 1024]).unwrap();
            source
        })
        .collect::<Vec<_>>();
    let mut job = transfer_job(&data, sources.clone(), &target, false);
    let progress = Progress::default();
    let plan = prepare(&data, &job, &progress).unwrap();
    job.plan = Some(plan.clone());
    save(&data, &job).unwrap();
    let expected = plan
        .iter()
        .map(|item| {
            (
                path(item.destination.as_ref().unwrap()).unwrap(),
                item.hash.clone(),
            )
        })
        .collect();
    undo::durable(&data, &job.id, expected, "Transfer\t64", &progress, || {
        progress.begin(Phase::Copying, Some(64 * 1025));
        // Exercise the worker pool even on CI filesystems with unknown hardware.
        parallel(plan.len(), 4, |index| {
            transfer(&plan[index], false, true, &progress)
        })
    })
    .unwrap();
    assert_eq!(progress.snapshot().files_done, 64);
    assert_eq!(progress.snapshot().bytes_done, 64 * 1024);
    for source in &sources {
        assert_eq!(
            fs::read(source).unwrap(),
            fs::read(target.join(source.file_name().unwrap())).unwrap()
        );
    }
    execute(&data, job, &Progress::default()).unwrap();
    assert!(undo::undo(&data).unwrap());
    assert!(fs::read_dir(target).unwrap().next().is_none());
    assert!(!undo::undo(&data).unwrap());
}

#[test]
fn cancellation_after_publishing_an_item_keeps_its_global_undo() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let target = root.path().join("target");
    fs::create_dir(&target).unwrap();
    let sources = ["one", "two"].map(|name| {
        let source = root.path().join(name);
        fs::write(&source, name).unwrap();
        source
    });
    let mut job = transfer_job(&data, sources.to_vec(), &target, true);
    let progress = Progress::default();
    let plan = prepare(&data, &job, &progress).unwrap();
    job.plan = Some(plan.clone());
    save(&data, &job).unwrap();
    let expected = plan
        .iter()
        .flat_map(|item| {
            [
                (path(&item.source).unwrap(), "-".into()),
                (
                    path(item.destination.as_ref().unwrap()).unwrap(),
                    item.hash.clone(),
                ),
            ]
        })
        .collect();
    let result = undo::durable(&data, &job.id, expected, "Transfer\t2", &progress, || {
        progress.begin(Phase::Moving, None);
        transfer(&plan[0], true, true, &progress)?;
        progress.cancel();
        progress.checkpoint()
    });
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
    execute(&data, job, &Progress::default()).unwrap();
    assert!(!sources[0].exists());
    assert!(sources[1].exists());
    assert!(undo::undo(&data).unwrap());
    assert!(sources.iter().all(|source| source.exists()));
    assert!(fs::read_dir(target).unwrap().next().is_none());
}

#[test]
fn undo_confirmation_cannot_restore_a_different_operation() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let target = root.path().join("target");
    fs::create_dir(&target).unwrap();
    let sources = ["one", "two"].map(|name| {
        let source = root.path().join(name);
        fs::write(&source, name).unwrap();
        source
    });
    let first = transfer_job(&data, vec![sources[0].clone()], &target, false);
    execute(&data, first, &Progress::default()).unwrap();
    let (key, _) = undo::latest_summary(&data).unwrap().unwrap();
    let second = transfer_job(&data, vec![sources[1].clone()], &target, false);
    execute(&data, second, &Progress::default()).unwrap();
    assert!(
        undo::undo_expected(&data, &key)
            .unwrap_err()
            .to_string()
            .contains("history changed")
    );
    assert!(target.join("one").exists());
    assert!(target.join("two").exists());
    assert!(undo::undo(&data).unwrap());
    assert!(undo::undo_expected(&data, &key).unwrap());
    assert!(fs::read_dir(target).unwrap().next().is_none());
}

#[test]
fn resumes_complete_read_only_staged_files_after_interruption() {
    let root = crash_fixture("readonly");
    let data = root.path().join("data");
    let job = recover(&data).unwrap().pop().unwrap();
    execute(&data, job, &Progress::default()).unwrap();
    assert_eq!(
        fs::metadata(root.path().join("target/first"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o440
    );
    assert_eq!(
        fs::read(root.path().join("target/first")).unwrap(),
        fs::read(root.path().join("first")).unwrap()
    );
}

#[test]
fn resumes_atomic_source_removal_after_a_staged_move() {
    let root = crash_fixture("parked-move");
    let data = root.path().join("data");
    let job = recover(&data).unwrap().pop().unwrap();
    let parked = path(job.plan.as_ref().unwrap()[0].parked.as_ref().unwrap()).unwrap();
    assert!(parked.join("contents").exists());
    execute(&data, job, &Progress::default()).unwrap();
    assert!(!parked.exists());
    assert!(!root.path().join("first").exists());
    assert!(undo::undo(&data).unwrap());
    assert_eq!(
        fs::read(root.path().join("first")).unwrap(),
        vec![42; 5 * 1024 * 1024 + 7]
    );
    assert_eq!(fs::read(root.path().join("second")).unwrap(), b"second");
}

#[test]
fn duplicate_detection_compares_contents_even_with_different_permissions() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let target = root.path().join("target");
    fs::create_dir(&target).unwrap();
    let source = root.path().join("file");
    fs::write(&source, b"same contents").unwrap();
    fs::write(target.join("file"), b"same contents").unwrap();
    fs::set_permissions(&source, fs::Permissions::from_mode(0o600)).unwrap();
    fs::set_permissions(target.join("file"), fs::Permissions::from_mode(0o440)).unwrap();
    let job = transfer_job(&data, vec![source.clone()], &target, false);
    let progress = Progress::default();
    std::thread::scope(|scope| {
        let task = scope.spawn(|| execute(&data, job, &progress));
        assert!(wait_conflict(&progress).identical);
        progress.resolve(Resolution::Skip, false);
        task.join().unwrap().unwrap();
    });
    assert!(source.exists());
    assert_eq!(
        fs::metadata(target.join("file"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o440
    );
}

#[test]
fn io_failure_retains_partial_data_for_retry_and_one_global_undo() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let target = root.path().join("target");
    fs::create_dir(&target).unwrap();
    let sources = ["one", "two"].map(|name| {
        let source = root.path().join(name);
        fs::write(&source, vec![17; 1024 * 1024]).unwrap();
        source
    });
    let mut job = transfer_job(&data, sources.to_vec(), &target, false);
    let progress = Progress::default();
    let plan = prepare(&data, &job, &progress).unwrap();
    job.plan = Some(plan.clone());
    save(&data, &job).unwrap();
    let expected = plan
        .iter()
        .map(|item| {
            (
                path(item.destination.as_ref().unwrap()).unwrap(),
                item.hash.clone(),
            )
        })
        .collect();
    let error = undo::durable(&data, &job.id, expected, "Transfer\t2", &progress, || {
        progress.begin(Phase::Copying, None);
        transfer(&plan[0], false, true, &progress)?;
        let staged = path(plan[1].staging.as_ref().unwrap())?.join("contents");
        fs::write(staged, vec![17; 128 * 1024])?;
        Err(io::Error::from_raw_os_error(libc::ENOSPC))
    })
    .unwrap_err();
    assert_eq!(error.raw_os_error(), Some(libc::ENOSPC));
    assert!(!undo::finalized(&data, &job.id).unwrap());
    assert!(undo::undo(&data).is_err());
    let recovered = recover(&data).unwrap().pop().unwrap();
    execute(&data, recovered, &Progress::default()).unwrap();
    for source in &sources {
        assert_eq!(
            fs::read(source).unwrap(),
            fs::read(target.join(source.file_name().unwrap())).unwrap()
        );
    }
    assert!(undo::undo(&data).unwrap());
    assert!(fs::read_dir(target).unwrap().next().is_none());
    assert!(!undo::undo(&data).unwrap());
}

#[test]
fn another_window_cannot_cancel_an_active_job_or_replay_a_cancelled_job() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let target = root.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::write(root.path().join("one"), b"one").unwrap();
    fs::write(root.path().join("two"), b"two").unwrap();
    let first = transfer_job(&data, vec![root.path().join("one")], &target, false);
    let second = transfer_job(&data, vec![root.path().join("two")], &target, false);
    let active = job_lock(&data, &first).unwrap();
    assert!(
        forget(&data, &first)
            .unwrap_err()
            .to_string()
            .contains("active")
    );
    forget(&data, &second).unwrap();
    assert!(execute(&data, second, &Progress::default()).is_err());
    assert!(!target.join("two").exists());
    assert_eq!(recover(&data).unwrap().len(), 1);
    drop(active);
    execute(&data, first, &Progress::default()).unwrap();
    assert_eq!(fs::read(target.join("one")).unwrap(), b"one");
}

#[test]
fn skipping_a_job_preserves_all_twenty_previous_undo_actions() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    for index in 0..20 {
        undo::execute(
            &data,
            Operation::New {
                directory: root.path().into(),
                name: format!("created-{index}"),
                folder: false,
            },
        )
        .unwrap();
    }
    let source = root.path().join("source");
    let target = root.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::write(&source, b"same").unwrap();
    fs::write(target.join("source"), b"same").unwrap();
    let job = transfer_job(&data, vec![source], &target, false);
    let progress = Progress::default();
    std::thread::scope(|scope| {
        let task = scope.spawn(|| execute(&data, job, &progress));
        wait_conflict(&progress);
        progress.resolve(Resolution::Skip, true);
        task.join().unwrap().unwrap();
    });
    for _ in 0..20 {
        assert!(undo::undo(&data).unwrap());
    }
    assert!(!undo::undo(&data).unwrap());
}
