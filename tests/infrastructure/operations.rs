use super::*;
#[test]
fn mutations_preserve_links_refuse_collisions_and_change_extensions() {
    let root = std::env::temp_dir().join(format!("virial-operations-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let source = root.join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("file.txt"), "hello").unwrap();
    symlink("missing", source.join("link")).unwrap();
    let destination = root.join("destination");
    fs::create_dir(&destination).unwrap();
    execute(Operation::Transfer {
        sources: vec![source.clone()],
        directory: destination.clone(),
        cut: false,
    })
    .unwrap();
    assert_eq!(
        fs::read(destination.join("source/file.txt")).unwrap(),
        b"hello"
    );
    assert_eq!(
        fs::read_link(destination.join("source/link")).unwrap(),
        Path::new("missing")
    );
    assert!(
        execute(Operation::Transfer {
            sources: vec![source.clone()],
            directory: destination.clone(),
            cut: false
        })
        .is_err()
    );
    assert!(
        execute(Operation::Transfer {
            sources: vec![source.clone()],
            directory: source.clone(),
            cut: false
        })
        .is_err()
    );
    execute(Operation::Rename {
        source: source.join("file.txt"),
        name: "renamed.md".into(),
    })
    .unwrap();
    assert!(source.join("renamed.md").exists());
    assert!(named_path(&root, "../escape").is_err());
    assert!(named_path(&root, ".").is_err());
    assert!(named_path(&root, "a/b").is_err());
    execute(Operation::New {
        directory: source.clone(),
        name: "new.txt".into(),
        folder: false,
    })
    .unwrap();
    assert!(
        execute(Operation::Rename {
            source: source.join("renamed.md"),
            name: "new.txt".into()
        })
        .is_err()
    );
    execute(Operation::Transfer {
        sources: vec![source.join("renamed.md")],
        directory: destination.clone(),
        cut: true,
    })
    .unwrap();
    assert!(!source.join("renamed.md").exists());
    execute(Operation::Compress(source.clone())).unwrap();
    assert!(root.join("source.tar.gz").exists());
    assert!(execute(Operation::Compress(source)).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn batch_move_carries_folders_links_and_selected_descendants_once() {
    let root = std::env::temp_dir().join(format!("virial-batch-move-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let source = root.join("source");
    let destination = root.join("destination");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&destination).unwrap();
    fs::create_dir(source.join("folder")).unwrap();
    fs::write(source.join("folder/nested.txt"), "nested").unwrap();
    fs::write(source.join("file.txt"), "file").unwrap();
    symlink("missing", source.join("link")).unwrap();
    execute(Operation::Transfer {
        sources: vec![
            source.join("folder/nested.txt"),
            source.join("file.txt"),
            source.join("folder"),
            source.join("link"),
            source.join("file.txt"),
        ],
        directory: destination.clone(),
        cut: true,
    })
    .unwrap();
    assert_eq!(
        fs::read(destination.join("folder/nested.txt")).unwrap(),
        b"nested"
    );
    assert_eq!(fs::read(destination.join("file.txt")).unwrap(), b"file");
    assert_eq!(
        fs::read_link(destination.join("link")).unwrap(),
        Path::new("missing")
    );
    assert!(!destination.join("nested.txt").exists());
    assert_eq!(fs::read_dir(&source).unwrap().count(), 0);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn batch_preflight_refuses_collisions_and_duplicate_names_before_any_move() {
    let root = std::env::temp_dir().join(format!("virial-batch-collision-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let source = root.join("source");
    let destination = root.join("destination");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&destination).unwrap();
    fs::write(source.join("a.txt"), "a").unwrap();
    fs::write(source.join("b.txt"), "b").unwrap();
    symlink("missing", destination.join("b.txt")).unwrap();
    let error = execute(Operation::Transfer {
        sources: vec![source.join("a.txt"), source.join("b.txt")],
        directory: destination.clone(),
        cut: true,
    })
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    assert!(source.join("a.txt").exists());
    assert!(source.join("b.txt").exists());
    assert!(!destination.join("a.txt").exists());
    assert_eq!(
        fs::read_link(destination.join("b.txt")).unwrap(),
        Path::new("missing")
    );
    fs::write(root.join("a.txt"), "different").unwrap();
    let error = execute(Operation::Transfer {
        sources: vec![source.join("a.txt"), root.join("a.txt")],
        directory: destination.clone(),
        cut: false,
    })
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    assert!(!destination.join("a.txt").exists());
    let error = execute(Operation::Transfer {
        sources: vec![source.join("a.txt"), source.join("missing.txt")],
        directory: destination.clone(),
        cut: true,
    })
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    assert!(source.join("a.txt").exists());
    assert!(!destination.join("a.txt").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn transfers_check_canonical_ancestry_and_allow_moves_to_the_existing_parent() {
    let root = std::env::temp_dir().join(format!("virial-batch-ancestry-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let source = root.join("folder");
    fs::create_dir(&source).unwrap();
    fs::create_dir(source.join("child")).unwrap();
    fs::write(source.join("file.txt"), "original").unwrap();
    symlink(&source, root.join("alias")).unwrap();
    for cut in [false, true] {
        let error = execute(Operation::Transfer {
            sources: vec![source.clone()],
            directory: root.join("alias/child"),
            cut,
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(!source.join("child/folder").exists());
    }
    execute(Operation::Transfer {
        sources: vec![root.join("alias/file.txt"), source.join("file.txt")],
        directory: source.clone(),
        cut: true,
    })
    .unwrap();
    assert_eq!(fs::read(source.join("file.txt")).unwrap(), b"original");
    assert!(
        execute(Operation::Transfer {
            sources: vec![source.join("file.txt")],
            directory: source.clone(),
            cut: false,
        })
        .is_err()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn progress_reports_chunks_nested_items_and_preserves_links() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::create_dir(source.join("empty-folder")).unwrap();
    fs::write(source.join("empty"), []).unwrap();
    let contents = vec![42; 3 * 1024 * 1024 + 17];
    fs::write(source.join("large"), &contents).unwrap();
    symlink("missing", source.join("link")).unwrap();
    let total = super::super::progress::weight(&source).unwrap();
    assert_eq!(total, contents.len() as u64 + 5);
    let progress = super::super::progress::Progress::default();
    progress.begin(super::super::progress::Phase::Copying, Some(total));
    let target = root.path().join("target");
    copy_with_progress(&source, &target, Some(&progress)).unwrap();
    assert_eq!(progress.snapshot().completed, total);
    assert_eq!(fs::read(target.join("large")).unwrap(), contents);
    assert_eq!(
        fs::read_link(target.join("link")).unwrap(),
        Path::new("missing")
    );
}

#[test]
fn progress_copy_cleans_partial_directory_and_never_overwrites() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    fs::create_dir(&source).unwrap();
    let fifo = CString::new(source.join("fifo").as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    let target = root.path().join("target");
    let progress = super::super::progress::Progress::default();
    assert!(copy_with_progress(&source, &target, Some(&progress)).is_err());
    assert!(!target.exists());
    fs::write(&target, "existing").unwrap();
    assert!(copy_with_progress(&source, &target, Some(&progress)).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"existing");
}

#[test]
fn move_progress_includes_undo_and_remains_undoable() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let target = root.path().join("target");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&target).unwrap();
    fs::write(source.join("file"), "contents").unwrap();
    let progress = super::super::progress::Progress::default();
    super::super::undo::execute_with_progress(
        &root.path().join("data"),
        Operation::Transfer {
            sources: vec![source.clone()],
            directory: target.clone(),
            cut: true,
        },
        Some(&progress),
    )
    .unwrap();
    let state = progress.snapshot();
    assert_eq!(state.phase, super::super::progress::Phase::Finishing);
    assert_eq!(Some(state.completed), state.total);
    assert!(!source.exists());
    assert_eq!(fs::read(target.join("source/file")).unwrap(), b"contents");
    assert!(super::super::undo::undo(&root.path().join("data")).unwrap());
    assert_eq!(fs::read(source.join("file")).unwrap(), b"contents");
    assert!(!target.join("source").exists());
}

#[test]
fn kernel_copy_chunks_preserve_boundaries_and_progress() {
    use super::super::progress::Progress;
    for length in [0, COPY_CHUNK as usize, COPY_CHUNK as usize * 2 + 17] {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let destination = root.path().join("destination");
        let contents = (0..length)
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>();
        fs::write(&source, &contents).unwrap();
        let mut input = File::open(&source).unwrap();
        let mut output = File::create(&destination).unwrap();
        let progress = Progress::default();
        // Bypass the optional reflink so this exercises multiple kernel-copy
        // chunks (or std's fallback on filesystems that cannot offload copies).
        copy_chunks(&mut input, &mut output, length as u64, Some(&progress)).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), contents);
        assert_eq!(progress.snapshot().completed, length as u64);
    }
}

#[test]
fn copy_rejects_changed_lengths_and_bounds_written_bytes() {
    for (contents, expected, written) in [
        (b"grown".as_slice(), 2, b"gr".as_slice()),
        (b"short".as_slice(), 10, b"short".as_slice()),
    ] {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let target = root.path().join("target");
        fs::write(&source, contents).unwrap();
        let mut input = File::open(&source).unwrap();
        let mut output = File::create(&target).unwrap();
        assert!(copy_contents(&mut input, &mut output, expected, None).is_err());
        assert_eq!(fs::read(&target).unwrap(), written);
        assert_eq!(fs::read(&source).unwrap(), contents);
    }
}

#[test]
fn regular_file_open_rejects_fifo_without_waiting_for_a_writer() {
    use std::{sync::mpsc, time::Duration};
    let root = tempfile::tempdir().unwrap();
    let fifo = root.path().join("fifo");
    let name = CString::new(fifo.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    let (tx, rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        tx.send(open_regular_file(&fifo).map(|_| ())).unwrap();
    });
    let error = rx
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Unsupported);
    worker.join().unwrap();
}

#[test]
fn regular_file_open_does_not_follow_replaced_symlinks() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let link = root.path().join("link");
    fs::write(&source, b"contents").unwrap();
    symlink(&source, &link).unwrap();
    assert!(open_regular_file(&link).is_err());
    assert_eq!(
        open_regular_file(&source)
            .unwrap()
            .metadata()
            .unwrap()
            .len(),
        8
    );
}

#[test]
fn accelerated_copy_preserves_sparse_contents_permissions_and_independence() {
    use std::os::unix::fs::{FileExt, MetadataExt, PermissionsExt};
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    let file = File::create(&source).unwrap();
    let size = COPY_CHUNK * 3 + 17;
    file.set_len(size).unwrap();
    file.write_all_at(b"header", 0).unwrap();
    file.write_all_at(b"tail", size - 4).unwrap();
    file.set_permissions(fs::Permissions::from_mode(0o640))
        .unwrap();
    let progress = super::super::progress::Progress::default();
    copy_with_progress(&source, &destination, Some(&progress)).unwrap();
    assert_eq!(fs::metadata(&destination).unwrap().len(), size);
    assert_eq!(
        fs::metadata(&destination).unwrap().permissions().mode() & 0o777,
        0o640
    );
    assert_ne!(
        fs::metadata(&source).unwrap().ino(),
        fs::metadata(&destination).unwrap().ino()
    );
    let copied = fs::read(&destination).unwrap();
    assert_eq!(&copied[..6], b"header");
    assert!(copied[6..copied.len() - 4].iter().all(|byte| *byte == 0));
    assert_eq!(&copied[copied.len() - 4..], b"tail");
    assert_eq!(progress.snapshot().completed, size + 1);
    File::options()
        .write(true)
        .open(&source)
        .unwrap()
        .write_all_at(b"edited", 0)
        .unwrap();
    let mut header = [0; 6];
    File::open(&destination)
        .unwrap()
        .read_exact_at(&mut header, 0)
        .unwrap();
    assert_eq!(&header, b"header");
    assert!(copy_with_progress(&source, &destination, Some(&progress)).is_err());
    assert_eq!(fs::read(&destination).unwrap(), copied);
}

#[test]
fn zip_compression_preserves_nested_files_and_refuses_overwrite() {
    use crate::infrastructure::compression::{ArchiveFormat, destination};
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("folder");
    fs::create_dir_all(source.join("nested/empty")).unwrap();
    fs::write(source.join("nested/café.txt"), "hello").unwrap();
    execute(Operation::CompressAs {
        path: source.clone(),
        format: ArchiveFormat::Zip,
    })
    .unwrap();
    let target = destination(&source, ArchiveFormat::Zip).unwrap();
    let bytes = fs::read(&target).unwrap();
    let mut archive = zip::ZipArchive::new(std::fs::File::open(&target).unwrap()).unwrap();
    assert_eq!(archive.by_name("folder/nested/café.txt").unwrap().size(), 5);
    assert!(archive.by_name("folder/nested/empty/").unwrap().is_dir());
    assert!(
        execute(Operation::CompressAs {
            path: source,
            format: ArchiveFormat::Zip
        })
        .is_err()
    );
    assert_eq!(fs::read(target).unwrap(), bytes);
}

#[test]
fn duplicate_copies_a_directory_and_refuses_existing_destination() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("folder");
    let destination = root.path().join("folder (copy 1)");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("file.txt"), "original").unwrap();
    execute(Operation::Duplicate {
        source: source.clone(),
        destination: destination.clone(),
    })
    .unwrap();
    fs::write(source.join("file.txt"), "changed").unwrap();
    assert_eq!(
        fs::read_to_string(destination.join("file.txt")).unwrap(),
        "original"
    );
    assert!(
        execute(Operation::Duplicate {
            source,
            destination: destination.clone()
        })
        .is_err()
    );
    assert_eq!(
        fs::read_to_string(destination.join("file.txt")).unwrap(),
        "original"
    );
}

#[test]
fn tar_formats_create_readable_archives() {
    use crate::infrastructure::compression::{ArchiveFormat, destination};
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("--source.txt");
    fs::write(&source, "hello").unwrap();
    for format in [
        ArchiveFormat::TarGz,
        ArchiveFormat::TarXz,
        ArchiveFormat::TarBz2,
    ] {
        execute(Operation::CompressAs {
            path: source.clone(),
            format,
        })
        .unwrap();
        let output = std::process::Command::new("tar")
            .arg("-xOf")
            .arg(destination(&source, format).unwrap())
            .output()
            .unwrap();
        assert!(output.status.success(), "{format:?}");
        assert_eq!(output.stdout, b"hello");
    }
}
