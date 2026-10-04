use super::*;

#[test]
fn dropping_size_task_cancels_filesystem_work() {
    let cancelled = Arc::new(AtomicBool::new(false));
    let task = DirectorySizeTask {
        _task: Task::ready(()),
        cancelled: cancelled.clone(),
    };
    assert!(!cancelled.load(Ordering::Relaxed));
    drop(task);
    assert!(cancelled.load(Ordering::Relaxed));
}
