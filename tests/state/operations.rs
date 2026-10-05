use super::*;
use std::fs;

#[test]
fn prepared_move_can_reload_and_does_not_replay_after_completion() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let source = root.path().join("source");
    let target = root.path().join("target");
    fs::write(&source, b"contents").unwrap();
    fs::create_dir(&target).unwrap();
    let prepared = QueuedOperation::prepare(
        &data,
        Operation::Transfer {
            sources: vec![source.clone()],
            directory: target.clone(),
            cut: true,
        },
        true,
    )
    .unwrap();
    assert!(source.exists());
    assert!(!target.join("source").exists());
    let reloaded = prepared.clone().reload(&data).unwrap().unwrap();
    assert_eq!(
        reloaded.journal.as_ref().unwrap().id,
        prepared.journal.as_ref().unwrap().id
    );
    queue::execute(&data, reloaded.journal.unwrap(), &Progress::default()).unwrap();
    assert!(!source.exists());
    assert_eq!(fs::read(target.join("source")).unwrap(), b"contents");
    assert!(prepared.reload(&data).unwrap().is_none());
}

#[test]
fn missing_destination_fails_preparation_without_moving_or_journaling() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let source = root.path().join("source");
    fs::write(&source, b"contents").unwrap();
    let result = QueuedOperation::prepare(
        &data,
        Operation::Transfer {
            sources: vec![source.clone()],
            directory: root.path().join("missing"),
            cut: true,
        },
        true,
    );
    assert!(matches!(result, Err(error) if error.kind() == std::io::ErrorKind::NotFound));
    assert_eq!(fs::read(&source).unwrap(), b"contents");
    assert!(queue::recover(&data).unwrap().is_empty());
}

#[test]
fn forgotten_waiting_move_is_not_reloaded() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let target = root.path().join("target");
    let data = root.path().join("data");
    fs::write(&source, b"contents").unwrap();
    fs::create_dir(&target).unwrap();
    let prepared = QueuedOperation::prepare(
        &data,
        Operation::Transfer {
            sources: vec![source.clone()],
            directory: target.clone(),
            cut: true,
        },
        true,
    )
    .unwrap();
    queue::forget(&data, prepared.journal.as_ref().unwrap()).unwrap();
    assert!(prepared.reload(&data).unwrap().is_none());
    assert_eq!(fs::read(&source).unwrap(), b"contents");
    assert!(!target.join("source").exists());
}
