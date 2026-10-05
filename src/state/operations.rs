//! Queue orchestration; local transfers and Trash are journaled before starting.
use crate::{
    app::FileManager,
    infrastructure::{operations::Operation, progress::Progress, queue},
};
use gpui::Context;

#[derive(Clone)]
pub(crate) struct QueuedOperation {
    pub operation: Operation,
    pub journal: Option<queue::Job>,
}

impl QueuedOperation {
    fn prepare(
        data: &std::path::Path,
        operation: Operation,
        verify: bool,
    ) -> std::io::Result<Self> {
        let journal = if queue::supported(&operation) {
            Some(queue::enqueue(data, &operation, verify)?)
        } else {
            None
        };
        Ok(Self { operation, journal })
    }

    fn reload(mut self, data: &std::path::Path) -> std::io::Result<Option<Self>> {
        if let Some(job) = self.journal.as_ref() {
            self.journal = queue::recover(data)?
                .into_iter()
                .find(|candidate| candidate.id == job.id);
            if self.journal.is_none() {
                return Ok(None);
            }
        }
        Ok(Some(self))
    }
}

impl FileManager {
    pub(crate) fn enqueue_operation(&mut self, operation: Operation, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        // Path resolution and durable journal writes can block on a slow drive.
        // Serialize preparation without performing any filesystem I/O on the UI.
        self.busy = true;
        self.error = None;
        let data = self.data_home.clone();
        let verify = self.verify_transfers;
        let task = cx
            .background_executor()
            .spawn(async move { QueuedOperation::prepare(&data, operation, verify) });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |view, cx| {
                view.busy = false;
                let error = match result {
                    Ok(operation) => {
                        view.operation_queue.push_back(operation);
                        None
                    }
                    Err(error) => Some(error),
                };
                view.start_queued_operation(false, cx);
                if let Some(error) = error {
                    view.error = Some(format!(
                        "{}: {error}",
                        view.language.text("Operation failed")
                    ));
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn recover_operations(&mut self, cx: &mut Context<Self>) {
        self.busy = true;
        let data = self.data_home.clone();
        let task = cx
            .background_executor()
            .spawn(async move { queue::recover(&data) });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |view, cx| {
                view.busy = false;
                match result {
                    Ok(jobs) => {
                        for job in jobs {
                            match queue::operation(&job) {
                                Ok(operation) => view.operation_queue.push_back(QueuedOperation {
                                    operation,
                                    journal: Some(job),
                                }),
                                Err(error) => {
                                    view.error = Some(error.to_string());
                                    cx.notify();
                                    return;
                                }
                            }
                        }
                        // A restart does not start moving or deleting files without review.
                        view.start_queued_operation(true, cx);
                    }
                    Err(error) => {
                        view.error = Some(format!(
                            "{}: {error}",
                            view.language.text("Cannot recover operation queue")
                        ))
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn start_queued_operation(&mut self, paused: bool, cx: &mut Context<Self>) {
        self.start_with_control(paused, false, cx);
    }

    fn start_with_control(&mut self, paused: bool, cancelled: bool, cx: &mut Context<Self>) {
        if self.queue_running || self.busy || self.active_operation.is_some() {
            return;
        }
        let Some(operation) = self.operation_queue.pop_front() else {
            return;
        };
        self.active_operation = Some(operation.clone());
        self.queue_running = true;
        self.queue_failed = false;
        self.error = None;
        let progress = Progress::default();
        progress.configure_verification(operation.journal.as_ref().is_some_and(|job| job.verify));
        if paused {
            progress.toggle_pause();
        }
        if cancelled {
            progress.cancel();
        }
        self.transfer_progress = Some(progress.clone());
        let monitor = cx.background_executor().clone();
        let monitored = progress.clone();
        cx.spawn(async move |view, cx| {
            loop {
                monitor.timer(std::time::Duration::from_millis(100)).await;
                let running = view
                    .update(cx, |view, cx| {
                        cx.notify();
                        view.queue_running
                            && view
                                .transfer_progress
                                .as_ref()
                                .is_some_and(|progress| progress.same(&monitored))
                    })
                    .unwrap_or(false);
                if !running {
                    break;
                }
            }
        })
        .detach();
        let data = self.data_home.clone();
        let recent = match &operation.operation {
            Operation::Launch { file, .. } => Some(file.clone()),
            _ => None,
        };
        let cut_sources = match &operation.operation {
            Operation::Transfer {
                sources, cut: true, ..
            } => Some(sources.clone()),
            _ => None,
        };
        let worker_cut_sources = cut_sources.clone();
        let task = cx.background_executor().spawn(async move {
            let result = if let Some(job) = operation.journal {
                queue::execute(&data, job, &progress).map(|_| None)
            } else {
                crate::infrastructure::undo::execute_with_progress(
                    &data,
                    operation.operation,
                    Some(&progress),
                )
            };
            let retained = queue::recover(&data)
                .map(|jobs| jobs.into_iter().map(|job| job.id).collect::<Vec<_>>());
            let result = result.and_then(|extracted| {
                if let Some(path) = recent {
                    crate::infrastructure::recent::record(&data, &path)?;
                }
                Ok(extracted)
            });
            let cut_completed = result.is_ok()
                && worker_cut_sources.as_ref().is_some_and(|sources| {
                    sources.iter().all(|source| {
                        std::fs::symlink_metadata(source)
                            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
                    })
                });
            (result, retained, cut_completed)
        });
        cx.spawn(async move |view, cx| {
            let (result, retained, cut_completed) = task.await;
            let _ = view.update(cx, |view, cx| {
                view.queue_running = false;
                let pending = view
                    .active_operation
                    .as_ref()
                    .and_then(|operation| operation.journal.as_ref())
                    .is_some_and(|job| {
                        retained
                            .as_ref()
                            .map_or(true, |jobs| jobs.contains(&job.id))
                    });
                let cancelled = result
                    .as_ref()
                    .is_err_and(|error| error.kind() == std::io::ErrorKind::Interrupted);
                let failed = result.is_err() && !cancelled;
                view.refresh(cx);
                match result {
                    Ok(extracted) => {
                        if let Some(extracted) = extracted {
                            view.opened_archive_files.push(extracted);
                        }
                        if cut_sources.as_ref().is_some_and(|sources| {
                            view.clipboard
                                .as_ref()
                                .is_some_and(|(paths, cut)| *cut && paths == sources)
                        }) {
                            // Skipped move conflicts keep their sources available for another paste.
                            if cut_completed {
                                view.clipboard = None;
                            }
                        }
                    }
                    Err(error) if !cancelled => {
                        view.error = Some(format!(
                            "{}: {error}",
                            view.language.text("Operation failed")
                        ))
                    }
                    Err(_) => {}
                }
                if pending || failed {
                    view.queue_failed = true;
                } else {
                    view.active_operation = None;
                    view.transfer_progress = None;
                    view.start_queued_operation(false, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn retry_operation(&mut self, cx: &mut Context<Self>) {
        self.retry_with_control(false, cx);
    }

    pub(crate) fn cancel_failed_operation(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.queue_running {
            return;
        }
        if self
            .active_operation
            .as_ref()
            .is_some_and(|operation| operation.journal.is_none())
        {
            self.active_operation = None;
            self.transfer_progress = None;
            self.queue_failed = false;
            self.start_queued_operation(false, cx);
            cx.notify();
        } else {
            self.retry_with_control(true, cx);
        }
    }

    fn retry_with_control(&mut self, cancelled: bool, cx: &mut Context<Self>) {
        if self.queue_running || self.busy {
            return;
        }
        let Some(operation) = self.active_operation.clone() else {
            return;
        };
        self.busy = true;
        let data = self.data_home.clone();
        let task = cx
            .background_executor()
            .spawn(async move { operation.reload(&data) });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |view, cx| {
                view.busy = false;
                match result {
                    Ok(operation) => {
                        view.active_operation = None;
                        view.transfer_progress = None;
                        view.queue_failed = false;
                        if let Some(operation) = operation {
                            view.operation_queue.push_front(operation);
                            view.start_with_control(false, cancelled, cx);
                        } else {
                            view.start_queued_operation(false, cx);
                        }
                    }
                    Err(error) => view.error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn cancel_waiting(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(operation) = self.operation_queue.get(index).cloned() else {
            return;
        };
        self.busy = true;
        let data = self.data_home.clone();
        let task = cx.background_executor().spawn(async move {
            if let Some(job) = operation.journal {
                queue::forget(&data, &job)
            } else {
                Ok(())
            }
        });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |view, cx| {
                view.busy = false;
                let error = match result {
                    Ok(()) => {
                        view.operation_queue.remove(index);
                        None
                    }
                    Err(error) => Some(error),
                };
                view.start_queued_operation(false, cx);
                if let Some(error) = error {
                    view.error = Some(error.to_string());
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

#[cfg(test)]
#[path = "../../tests/state/operations.rs"]
mod tests;
