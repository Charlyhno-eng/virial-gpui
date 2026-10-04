use super::*;
use crate::infrastructure::{
    operations::{Operation, execute},
    storage,
};

fn fixture(path: &Path) {
    let mut writer = ZipWriter::new(File::create(path).unwrap());
    writer.set_comment("keep this comment");
    for (name, contents) in [
        ("implicit/code.rs", "fn main() {}"),
        ("implicit/.hidden", "secret"),
        ("note.txt", "hello"),
        ("destination/existing.txt", "existing"),
    ] {
        writer
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(contents.as_bytes()).unwrap();
    }
    writer
        .add_directory("empty/", SimpleFileOptions::default())
        .unwrap();
    writer.finish().unwrap();
}

#[test]
fn lists_implicit_empty_and_hidden_folders_and_reads_bounded_contents() {
    let root = tempfile::tempdir().unwrap();
    let archive = root.path().join("sample.ZIP");
    fixture(&archive);
    let entries = storage::read_directory(&archive, false).unwrap();
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        ["destination", "empty", "implicit", "note.txt"]
    );
    assert_eq!(
        storage::read_directory(&archive.join("implicit"), false)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        storage::read_directory(&archive.join("implicit"), true)
            .unwrap()
            .len(),
        2
    );
    assert!(
        storage::read_directory(&archive.join("empty"), true)
            .unwrap()
            .is_empty()
    );
    assert!(storage::read_directory(&archive.join("missing"), true).is_err());
    assert!(storage::read_directory(&archive.join("note.txt"), true).is_err());
    assert_eq!(read_prefix(&archive.join("note.txt"), 3).unwrap(), b"hel");
    assert_eq!(directory_size(&archive, Path::new("implicit")).unwrap(), 18);
    assert!(materialize(&archive.join("note.txt"), 4).is_err());
    let file = materialize(&archive.join("note.txt"), 5).unwrap();
    assert_eq!(fs::read(&file.path).unwrap(), b"hello");
    let path = file.path.clone();
    drop(file);
    assert!(!path.exists());
}

#[test]
fn renames_files_and_implicit_folder_trees_without_overwriting() {
    let root = tempfile::tempdir().unwrap();
    let archive = root.path().join("sample.zip");
    fixture(&archive);
    execute(Operation::Rename {
        source: archive.join("note.txt"),
        name: "renamed.md".into(),
    })
    .unwrap();
    execute(Operation::Rename {
        source: archive.join("implicit"),
        name: "renamed folder".into(),
    })
    .unwrap();
    assert_eq!(
        read_prefix(&archive.join("renamed.md"), 100).unwrap(),
        b"hello"
    );
    assert_eq!(
        read_prefix(&archive.join("renamed folder/code.rs"), 100).unwrap(),
        b"fn main() {}"
    );
    assert!(read_prefix(&archive.join("implicit/code.rs"), 100).is_err());
    let before = fs::read(&archive).unwrap();
    for name in ["empty", "../escape", "a/b", ""] {
        assert!(
            execute(Operation::Rename {
                source: archive.join("renamed folder"),
                name: name.into()
            })
            .is_err()
        );
        assert_eq!(fs::read(&archive).unwrap(), before);
    }
    assert_eq!(open(&archive).unwrap().comment(), b"keep this comment");
    // Renaming a real ZIP still renames the file on disk.
    execute(Operation::Rename {
        source: archive.clone(),
        name: "renamed.zip".into(),
    })
    .unwrap();
    assert!(!archive.exists());
    assert!(root.path().join("renamed.zip").exists());
}

#[test]
fn moves_and_copies_within_zip_and_preserves_empty_source_folders() {
    let root = tempfile::tempdir().unwrap();
    let archive = root.path().join("sample.zip");
    fixture(&archive);
    execute(Operation::Transfer {
        sources: vec![archive.join("implicit"), archive.join("implicit/code.rs")],
        directory: archive.join("destination"),
        cut: true,
    })
    .unwrap();
    assert!(
        storage::read_directory(&archive, true)
            .unwrap()
            .iter()
            .all(|entry| entry.name != "implicit")
    );
    assert_eq!(
        read_prefix(&archive.join("destination/implicit/code.rs"), 100).unwrap(),
        b"fn main() {}"
    );
    execute(Operation::Transfer {
        sources: vec![archive.join("note.txt")],
        directory: archive.join("empty"),
        cut: false,
    })
    .unwrap();
    assert_eq!(
        read_prefix(&archive.join("note.txt"), 100).unwrap(),
        b"hello"
    );
    assert_eq!(
        read_prefix(&archive.join("empty/note.txt"), 100).unwrap(),
        b"hello"
    );
    let before = fs::read(&archive).unwrap();
    assert!(
        execute(Operation::Transfer {
            sources: vec![archive.join("destination")],
            directory: archive.join("destination/implicit"),
            cut: true
        })
        .is_err()
    );
    assert_eq!(fs::read(&archive).unwrap(), before);
    execute(Operation::Transfer {
        sources: vec![archive.join("empty/note.txt")],
        directory: archive.join("destination"),
        cut: true,
    })
    .unwrap();
    assert_eq!(
        read_prefix(&archive.join("destination/note.txt"), 100).unwrap(),
        b"hello"
    );
    assert!(
        storage::read_directory(&archive.join("empty"), true)
            .unwrap()
            .is_empty()
    );
    execute(Operation::New {
        directory: archive.join("empty"),
        name: "new folder".into(),
        folder: true,
    })
    .unwrap();
    execute(Operation::New {
        directory: archive.join("empty/new folder"),
        name: "new.txt".into(),
        folder: false,
    })
    .unwrap();
    assert_eq!(
        read_prefix(&archive.join("empty/new folder/new.txt"), 100).unwrap(),
        b""
    );
}

#[test]
fn transfers_between_disk_and_archives_and_refuses_batch_collisions() {
    let root = tempfile::tempdir().unwrap();
    let first = root.path().join("first.zip");
    let second = root.path().join("second.zip");
    fixture(&first);
    fixture(&second);
    let local = root.path().join("local");
    fs::create_dir(&local).unwrap();
    fs::write(local.join("import.txt"), "imported").unwrap();
    execute(Operation::Transfer {
        sources: vec![local.join("import.txt")],
        directory: first.clone(),
        cut: true,
    })
    .unwrap();
    assert!(!local.join("import.txt").exists());
    execute(Operation::Transfer {
        sources: vec![first.join("import.txt")],
        directory: second.join("empty"),
        cut: true,
    })
    .unwrap();
    assert!(read_prefix(&first.join("import.txt"), 100).is_err());
    assert_eq!(
        read_prefix(&second.join("empty/import.txt"), 100).unwrap(),
        b"imported"
    );
    execute(Operation::Transfer {
        sources: vec![second.join("empty"), second.join("empty/import.txt")],
        directory: local.clone(),
        cut: true,
    })
    .unwrap();
    assert_eq!(
        fs::read(local.join("empty/import.txt")).unwrap(),
        b"imported"
    );
    assert!(
        storage::read_directory(&second, true)
            .unwrap()
            .iter()
            .all(|entry| entry.name != "empty")
    );
    fs::write(local.join("note.txt"), "collision").unwrap();
    let before = fs::read(&first).unwrap();
    assert!(
        execute(Operation::Transfer {
            sources: vec![first.join("implicit"), first.join("note.txt")],
            directory: local.clone(),
            cut: true
        })
        .is_err()
    );
    assert!(!local.join("implicit").exists());
    assert_eq!(fs::read(&first).unwrap(), before);
    assert!(
        execute(Operation::Transfer {
            sources: vec![first.clone()],
            directory: first.clone(),
            cut: false
        })
        .is_err()
    );
    assert!(
        execute(Operation::Transfer {
            sources: vec![first.join("missing")],
            directory: second,
            cut: true
        })
        .is_err()
    );
    assert_eq!(fs::read(&first).unwrap(), before);
}

#[test]
fn refuses_unsafe_archives_and_links_without_changing_originals() {
    let root = tempfile::tempdir().unwrap();
    for (index, name) in [
        "../escape",
        "/absolute",
        "folder/../../escape",
        "folder\\escape",
    ]
    .iter()
    .enumerate()
    {
        let archive = root.path().join(format!("unsafe-{index}.zip"));
        let mut writer = ZipWriter::new(File::create(&archive).unwrap());
        writer
            .start_file(*name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"unsafe").unwrap();
        writer.finish().unwrap();
        let before = fs::read(&archive).unwrap();
        assert!(storage::read_directory(&archive, true).is_err());
        assert!(rename(&archive.join("folder"), "renamed").is_err());
        assert_eq!(fs::read(&archive).unwrap(), before);
    }
    let archive = root.path().join("links.zip");
    let mut writer = ZipWriter::new(File::create(&archive).unwrap());
    writer
        .add_symlink("link", "../../escape", SimpleFileOptions::default())
        .unwrap();
    writer.finish().unwrap();
    let before = fs::read(&archive).unwrap();
    assert!(
        execute(Operation::Transfer {
            sources: vec![archive.join("link")],
            directory: root.path().to_path_buf(),
            cut: true
        })
        .is_err()
    );
    assert!(!root.path().join("link").exists());
    assert_eq!(fs::read(&archive).unwrap(), before);
    std::os::unix::fs::symlink("missing", root.path().join("local-link")).unwrap();
    assert!(
        execute(Operation::Transfer {
            sources: vec![root.path().join("local-link")],
            directory: archive.clone(),
            cut: true
        })
        .is_err()
    );
    assert_eq!(fs::read(&archive).unwrap(), before);
    assert!(fs::symlink_metadata(root.path().join("local-link")).is_ok());
    assert!(rename(&archive.join("link"), "renamed").is_err());
    fs::write(root.path().join("regular.txt"), "regular").unwrap();
    assert!(
        execute(Operation::Transfer {
            sources: vec![root.path().join("regular.txt")],
            directory: archive.clone(),
            cut: true,
        })
        .is_err()
    );
    assert_eq!(fs::read(&archive).unwrap(), before);
    assert!(root.path().join("regular.txt").exists());
}

#[test]
fn refuses_encrypted_records_and_concurrent_archive_changes() {
    let root = tempfile::tempdir().unwrap();
    let archive = root.path().join("sample.zip");
    fixture(&archive);
    let mut bytes = fs::read(&archive).unwrap();
    // Mark a record encrypted in both headers. Raw copying must never strip this flag.
    let local = bytes
        .windows(4)
        .position(|bytes| bytes == b"PK\x03\x04")
        .unwrap();
    let central = bytes
        .windows(4)
        .position(|bytes| bytes == b"PK\x01\x02")
        .unwrap();
    bytes[local + 6] |= 1;
    bytes[central + 8] |= 1;
    fs::write(&archive, &bytes).unwrap();
    assert!(rename(&archive.join("note.txt"), "renamed.txt").is_err());
    assert_eq!(fs::read(&archive).unwrap(), bytes);

    fixture(&archive);
    let original = fs::metadata(&archive).unwrap();
    let mut bytes = fs::read(&archive).unwrap();
    bytes.push(0);
    fs::write(&archive, &bytes).unwrap();
    assert!(rewrite(&archive, &original, |path| vec![path.to_path_buf()], &[]).is_err());
    assert_eq!(fs::read(&archive).unwrap(), bytes);
}

#[test]
fn recent_files_resolve_zip_members_and_skip_removed_members() {
    let root = tempfile::tempdir().unwrap();
    let archive = root.path().join("sample.zip");
    fixture(&archive);
    let path = archive.join("note.txt");
    crate::infrastructure::recent::record(root.path(), &path).unwrap();
    let entries = crate::infrastructure::recent::read(root.path(), true).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, path);
    assert_eq!(entries[0].bytes, Some(5));
    rename(&path, "renamed.txt").unwrap();
    assert!(
        crate::infrastructure::recent::read(root.path(), true)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn preserves_archive_permissions_refuses_read_only_writes_and_resolves_aliases() {
    let root = tempfile::tempdir().unwrap();
    let archive = root.path().join("sample.zip");
    fixture(&archive);
    fs::set_permissions(&archive, fs::Permissions::from_mode(0o640)).unwrap();
    let alias = root.path().join("alias.zip");
    std::os::unix::fs::symlink(&archive, &alias).unwrap();
    execute(Operation::Transfer {
        sources: vec![alias.join("note.txt")],
        directory: archive.join("empty"),
        cut: true,
    })
    .unwrap();
    assert_eq!(
        read_prefix(&alias.join("empty/note.txt"), 100).unwrap(),
        b"hello"
    );
    assert!(fs::symlink_metadata(&alias).unwrap().is_symlink());
    assert_eq!(
        fs::metadata(&archive).unwrap().permissions().mode() & 0o777,
        0o640
    );
    fs::set_permissions(&archive, fs::Permissions::from_mode(0o444)).unwrap();
    let before = fs::read(&archive).unwrap();
    assert!(rename(&archive.join("empty/note.txt"), "renamed.txt").is_err());
    assert_eq!(fs::read(&archive).unwrap(), before);
    assert_eq!(
        read_prefix(&archive.join("empty/note.txt"), 100).unwrap(),
        b"hello"
    );
    fs::set_permissions(&archive, fs::Permissions::from_mode(0o640)).unwrap();
}

#[test]
fn refuses_duplicate_member_names_without_discarding_records() {
    let root = tempfile::tempdir().unwrap();
    let archive = root.path().join("duplicate.zip");
    let mut writer = ZipWriter::new(File::create(&archive).unwrap());
    for name in ["first.txt", "other.txt"] {
        writer
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(name.as_bytes()).unwrap();
    }
    writer.finish().unwrap();
    let mut bytes = fs::read(&archive).unwrap();
    for index in 0..bytes.len() - 8 {
        if &bytes[index..index + 9] == b"other.txt" {
            bytes[index..index + 9].copy_from_slice(b"first.txt");
        }
    }
    fs::write(&archive, &bytes).unwrap();
    assert!(storage::read_directory(&archive, true).is_err());
    assert!(rename(&archive.join("first.txt"), "renamed.txt").is_err());
    assert_eq!(fs::read(&archive).unwrap(), bytes);
}
