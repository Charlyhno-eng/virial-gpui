use crate::infrastructure::{operations::Operation, progress::Progress, queue, undo};
use std::fs;

#[test]
#[cfg(windows)]
fn undo_durable_probe() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    fs::create_dir_all(&data).unwrap();
    let source = root.path().join("source");
    fs::write(&source, b"contents").unwrap();
    let result = undo::durable(
        &data,
        "probe-job",
        vec![(source.clone(), "-".into())],
        "Transfer\t1",
        &Progress::default(),
        || {
            fs::rename(&source, root.path().join("moved")).unwrap();
            Ok(())
        },
    );
    match result {
        Ok(_) => println!("undo durable ok"),
        Err(error) => panic!("undo durable failed: {error:?}"),
    }
}

#[test]
#[cfg(windows)]
fn execute_probe() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let source = root.path().join("source");
    let target = root.path().join("target");
    fs::write(&source, b"contents").unwrap();
    fs::create_dir(&target).unwrap();
    let operation = Operation::Transfer {
        sources: vec![source.clone()],
        directory: target.clone(),
        cut: true,
    };
    let job = queue::enqueue(&data, &operation, true).unwrap();
    match queue::execute(&data, job, &Progress::default()) {
        Ok(()) => println!("execute ok"),
        Err(error) => panic!("execute failed: {error:?}"),
    }
    assert!(!source.exists(), "moved source must be gone");
    assert_eq!(fs::read(target.join("source")).unwrap(), b"contents");
}
