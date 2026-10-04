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
    execute(Operation::Paste {
        source: source.clone(),
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
        execute(Operation::Paste {
            source: source.clone(),
            directory: destination.clone(),
            cut: false
        })
        .is_err()
    );
    assert!(
        execute(Operation::Paste {
            source: source.clone(),
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
    execute(Operation::Paste {
        source: source.join("renamed.md"),
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
