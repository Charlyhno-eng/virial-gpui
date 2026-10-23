use super::*;
use crate::infrastructure::{operations::Operation, undo};

#[test]
fn optional_colors_persist_and_toggle_independently_for_batches() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let folder = root.path().join("A folder");
    fs::create_dir(&folder).unwrap();
    let file = folder.join("Résumé #1.txt");
    fs::write(&file, "untouched").unwrap();
    assert!(read(&data).unwrap().0.is_empty());
    let paths = vec![file.clone(), folder.clone()];
    set(&data, &paths, Some(Tag::Red), true).unwrap();
    set(&data, std::slice::from_ref(&file), Some(Tag::Blue), true).unwrap();
    set(&data, &paths, Some(Tag::Red), true).unwrap();
    let tags = read(&data).unwrap();
    assert_eq!(tags.colors(&file).len(), 2);
    assert_eq!(tags.colors(&folder), &[Tag::Red]);
    let entries = tags.entries(Tag::Red).unwrap();
    assert_eq!(entries.len(), 2);
    assert!(entries[0].directory);
    assert_eq!(entries[1].path, file);
    set(&data, &paths, Some(Tag::Red), false).unwrap();
    assert_eq!(read(&data).unwrap().colors(&file), &[Tag::Blue]);
    assert!(read(&data).unwrap().colors(&folder).is_empty());
    set(&data, &paths, None, false).unwrap();
    assert!(read(&data).unwrap().0.is_empty());
    assert_eq!(fs::read_to_string(&file).unwrap(), "untouched");
}

#[test]
fn unavailable_items_are_hidden_without_losing_labels() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let file = root.path().join("file");
    fs::write(&file, "hello").unwrap();
    set(&data, std::slice::from_ref(&file), Some(Tag::Green), true).unwrap();
    fs::remove_file(&file).unwrap();
    let tags = read(&data).unwrap();
    assert!(tags.entries(Tag::Green).unwrap().is_empty());
    assert_eq!(tags.colors(&file), &[Tag::Green]);
    fs::write(&file, "hello").unwrap();
    assert_eq!(tags.entries(Tag::Green).unwrap().len(), 1);
}

#[test]
fn invalid_batches_and_corrupt_storage_do_not_overwrite_labels() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let file = root.path().join("file");
    fs::write(&file, "hello").unwrap();
    set(&data, std::slice::from_ref(&file), Some(Tag::Red), true).unwrap();
    let saved = fs::read(data.join("virial/tags.json")).unwrap();
    assert!(
        set(
            &data,
            &[file.clone(), root.path().join("missing")],
            Some(Tag::Pink),
            true
        )
        .is_err()
    );
    assert_eq!(fs::read(data.join("virial/tags.json")).unwrap(), saved);
    fs::write(data.join("virial/tags.json"), "broken").unwrap();
    assert!(set(&data, &[file], Some(Tag::Pink), true).is_err());
    assert_eq!(
        fs::read_to_string(data.join("virial/tags.json")).unwrap(),
        "broken"
    );
}

#[test]
fn relocation_updates_descendants_without_affecting_neighbor_folders() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let folder = root.path().join("folder");
    let neighbor = root.path().join("folder-other");
    fs::create_dir(&folder).unwrap();
    fs::create_dir(&neighbor).unwrap();
    let file = folder.join("nested");
    fs::write(&file, "hello").unwrap();
    set(
        &data,
        &[folder.clone(), file, neighbor.clone()],
        Some(Tag::Purple),
        true,
    )
    .unwrap();
    let moved = root.path().join("moved");
    fs::rename(&folder, &moved).unwrap();
    relocate(&data, &folder, &moved).unwrap();
    let tags = read(&data).unwrap();
    assert!(tags.colors(&folder).is_empty());
    for path in [&moved, &moved.join("nested"), &neighbor] {
        assert_eq!(tags.colors(path), &[Tag::Purple]);
    }
}

#[cfg(unix)]
#[test]
fn rename_move_and_undo_keep_folder_and_descendant_tags() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let folder = root.path().join("original");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("nested"), "hello").unwrap();
    set(
        &data,
        &[folder.clone(), folder.join("nested")],
        Some(Tag::Orange),
        true,
    )
    .unwrap();
    undo::execute(
        &data,
        Operation::Rename {
            source: folder.clone(),
            name: "renamed".into(),
        },
    )
    .unwrap();
    let renamed = root.path().join("renamed");
    assert_eq!(
        read(&data).unwrap().colors(&renamed.join("nested")),
        &[Tag::Orange]
    );
    let destination = root.path().join("destination");
    fs::create_dir(&destination).unwrap();
    undo::execute(
        &data,
        Operation::Transfer {
            sources: vec![renamed.clone()],
            directory: destination.clone(),
            cut: true,
        },
    )
    .unwrap();
    assert_eq!(
        read(&data)
            .unwrap()
            .colors(&destination.join("renamed/nested")),
        &[Tag::Orange]
    );
    undo::undo(&data).unwrap();
    assert_eq!(read(&data).unwrap().colors(&renamed), &[Tag::Orange]);
    undo::undo(&data).unwrap();
    assert_eq!(
        read(&data).unwrap().colors(&folder.join("nested")),
        &[Tag::Orange]
    );
    assert_eq!(read(&data).unwrap().entries(Tag::Orange).unwrap().len(), 2);
}

#[cfg(unix)]
#[test]
fn non_utf8_paths_round_trip_as_file_urls() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join(OsString::from_vec(vec![b'f', 0xff]));
    fs::write(&file, "hello").unwrap();
    set(
        root.path(),
        std::slice::from_ref(&file),
        Some(Tag::Gray),
        true,
    )
    .unwrap();
    assert_eq!(read(root.path()).unwrap().colors(&file), &[Tag::Gray]);
}

#[cfg(unix)]
#[test]
fn queued_conflict_renamed_move_and_undo_preserve_tags() {
    use crate::infrastructure::{
        progress::{Progress, Resolution},
        queue,
    };
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let file = root.path().join("notes.txt");
    let target = root.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::write(&file, "tagged").unwrap();
    fs::write(target.join("notes.txt"), "existing").unwrap();
    set(&data, std::slice::from_ref(&file), Some(Tag::Pink), true).unwrap();
    let job = queue::enqueue(
        &data,
        &Operation::Transfer {
            sources: vec![file.clone()],
            directory: target.clone(),
            cut: true,
        },
        false,
    )
    .unwrap();
    let progress = Progress::default();
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| queue::execute(&data, job, &progress));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while progress.snapshot().conflict.is_none() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        progress.resolve(Resolution::KeepBoth, true);
        worker.join().unwrap().unwrap();
    });
    let moved = target.join("notes (1).txt");
    assert_eq!(read(&data).unwrap().colors(&moved), &[Tag::Pink]);
    assert!(
        read(&data)
            .unwrap()
            .colors(&target.join("notes.txt"))
            .is_empty()
    );
    undo::undo(&data).unwrap();
    assert_eq!(read(&data).unwrap().colors(&file), &[Tag::Pink]);
    assert!(read(&data).unwrap().colors(&moved).is_empty());
}
