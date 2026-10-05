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

impl FileManager {
    pub(crate) fn enqueue_operation(&mut self, operation: Operation, cx: &mut Context<Self>) {
        let journal = if queue::supported(&operation) {
            match queue::enqueue(&self.data_home, &operation, self.verify_transfers) {
                Ok(job) => Some(job),
                Err(error) => {
                    self.error = Some(format!(
                        "{}: {error}",
                        self.language.text("Operation failed")
                    ));
                    cx.notify();
                    return;
                }
            }
        } else {
            None
        };
        self.operation_queue
            .push_back(QueuedOperation { operation, journal });
        self.start_queued_operation(false, cx);
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
            (result, retained)
        });
        cx.spawn(async move |view, cx| {
            let (result, retained) = task.await;
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
                            if cut_sources.as_ref().is_some_and(|sources| {
                                sources.iter().all(|source| {
                                    std::fs::symlink_metadata(source).is_err_and(|error| {
                                        error.kind() == std::io::ErrorKind::NotFound
                                    })
                                })
                            }) {
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
        if self.queue_running {
            return;
        }
        if let Some(mut operation) = self.active_operation.take() {
            // Read the durable plan written by the worker, including resolved names.
            if let Some(job) = operation.journal.as_ref() {
                match queue::recover(&self.data_home) {
                    Ok(jobs) => {
                        operation.journal =
                            jobs.into_iter().find(|candidate| candidate.id == job.id);
                        if operation.journal.is_none() {
                            self.transfer_progress = None;
                            self.queue_failed = false;
                            self.start_queued_operation(false, cx);
                            cx.notify();
                            return;
                        }
                    }
                    Err(error) => {
                        self.error = Some(error.to_string());
                        self.active_operation = Some(operation);
                        cx.notify();
                        return;
                    }
                }
            }
            self.operation_queue.push_front(operation);
        }
        self.start_with_control(false, cancelled, cx);
    }

    pub(crate) fn cancel_waiting(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(operation) = self.operation_queue.get(index)
            && let Some(job) = &operation.journal
        {
            if let Err(error) = queue::forget(&self.data_home, job) {
                self.error = Some(error.to_string());
                cx.notify();
                return;
            }
        }
        self.operation_queue.remove(index);
        cx.notify();
    }
}
