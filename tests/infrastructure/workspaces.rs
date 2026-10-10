use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "virial-workspaces-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn add(&self, name: &str, folder: Option<PathBuf>) {
        edit(
            &self.0,
            Edit::Add {
                name: name.into(),
                folder,
            },
        )
        .unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn persists_multiple_folders_and_deduplicates_canonical_paths() {
    let data = Fixture::new();
    assert!(read(&data.0).unwrap().is_empty());
    let one = data.0.join("src & docs\t");
    let two = data.0.join("architecture");
    fs::create_dir(&one).unwrap();
    fs::create_dir(&two).unwrap();
    data.add(" T.A.R.S. & équipe ", Some(one.clone()));
    data.add("T.A.R.S. & équipe", Some(one.join(".")));
    data.add("T.A.R.S. & équipe", Some(two.clone()));
    data.add("Empty", None);
    let saved = read(&data.0).unwrap();
    assert_eq!(saved.len(), 2);
    assert_eq!(saved[0].name, "T.A.R.S. & équipe");
    // macOS resolves /var through the /private/var symlink, so compare the
    // canonical form on both sides rather than the raw joined path.
    let expected = vec![
        one.canonicalize().unwrap_or(one),
        two.canonicalize().unwrap_or(two),
    ];
    assert_eq!(saved[0].folders, expected);
    assert!(saved[1].folders.is_empty());
}

#[test]
fn removes_only_associations_and_keeps_files() {
    let data = Fixture::new();
    let folder = data.0.join("project");
    fs::create_dir(&folder).unwrap();
    let file = folder.join("README.md");
    fs::write(&file, "keep me").unwrap();
    data.add("Project", Some(folder.canonicalize().unwrap()));
    let canonical = folder.canonicalize().unwrap();
    edit(
        &data.0,
        Edit::RemoveFolder {
            name: "Project".into(),
            folder: canonical,
        },
    )
    .unwrap();
    assert!(read(&data.0).unwrap()[0].folders.is_empty());
    data.add("Project", Some(folder.canonicalize().unwrap()));
    edit(&data.0, Edit::Remove("Project".into())).unwrap();
    assert!(read(&data.0).unwrap().is_empty());
    assert_eq!(fs::read_to_string(file).unwrap(), "keep me");
}

#[test]
fn rejects_invalid_edits_and_preserves_corrupt_configuration() {
    let data = Fixture::new();
    data.add("Keep", None);
    let target = data.0.join("virial/workspaces");
    let before = fs::read(&target).unwrap();
    for name in ["", "  ", "line\nbreak"] {
        assert!(
            edit(
                &data.0,
                Edit::Add {
                    name: name.into(),
                    folder: None
                }
            )
            .is_err()
        );
    }
    let file = data.0.join("file");
    fs::write(&file, "file").unwrap();
    assert!(
        edit(
            &data.0,
            Edit::Add {
                name: "Bad".into(),
                folder: Some(file)
            }
        )
        .is_err()
    );
    assert_eq!(fs::read(&target).unwrap(), before);
    fs::write(&target, "name=Keep&folder=https%3A%2F%2Fexample.com").unwrap();
    assert!(read(&data.0).is_err());
    assert!(edit(&data.0, Edit::Remove("Keep".into())).is_err());
    assert_eq!(
        fs::read_to_string(target).unwrap(),
        "name=Keep&folder=https%3A%2F%2Fexample.com"
    );
}

#[test]
fn summarizes_direct_contents_with_hidden_filter_sorted_activity_and_missing_roots() {
    let data = Fixture::new();
    let one = data.0.join("one");
    let two = data.0.join("two");
    fs::create_dir_all(one.join("nested")).unwrap();
    fs::create_dir(&two).unwrap();
    fs::write(one.join("nested/deep.rs"), "excluded").unwrap();
    fs::write(one.join(".hidden"), "hidden").unwrap();
    for index in 0..7 {
        let file = one.join(format!("{index}.rs"));
        fs::write(&file, "code").unwrap();
        fs::File::open(file)
            .unwrap()
            .set_times(
                fs::FileTimes::new()
                    .set_modified(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(index)),
            )
            .unwrap();
    }
    data.add("Project", Some(one.canonicalize().unwrap()));
    data.add("Project", Some(two.canonicalize().unwrap()));
    let two = two.canonicalize().unwrap();
    let one = one.canonicalize().unwrap();
    fs::remove_dir(&two).unwrap();
    let summary = summaries(&data.0, false).unwrap().remove(0);
    assert_eq!((summary.directories, summary.files), (1, 7));
    assert_eq!(summary.unavailable, vec![two]);
    assert_eq!(summary.recent.len(), 5);
    assert_eq!(summary.recent[0].1.path, one.join("6.rs"));
    assert_eq!(summary.recent[4].1.path, one.join("2.rs"));
    assert_eq!(summaries(&data.0, true).unwrap()[0].files, 8);
}

#[test]
fn canonicalizes_symlink_roots_and_does_not_duplicate_their_contents() {
    let data = Fixture::new();
    let root = data.0.join("root");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("a.rs"), "code").unwrap();
    let alias = data.0.join("alias");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&root, &alias).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&root, &alias).unwrap();
    data.add("Project", Some(root));
    data.add("Project", Some(alias));
    let summary = summaries(&data.0, false).unwrap().remove(0);
    assert_eq!(summary.workspace.folders.len(), 1);
    assert_eq!(summary.files, 1);
}
