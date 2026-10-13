//! Return freed allocator pages after the Linux desktop settles.
use crate::app::FileManager;
use gpui::{Context, Task};
use std::time::{Duration, Instant};

const SETTLE: Duration = Duration::from_secs(2);

pub(crate) struct IdleMemory {
    activity: Instant,
    task: Option<Task<()>>,
}

impl Default for IdleMemory {
    fn default() -> Self {
        Self {
            activity: Instant::now(),
            task: None,
        }
    }
}

pub(crate) fn release_unused_pages() {
    // glibc synchronizes its arenas internally. Only free pages are reclaimed;
    // live data, image caches and undo history are unaffected.
    unsafe { libc::malloc_trim(0) };
}

impl FileManager {
    pub(crate) fn reclaim_memory_when_idle(&mut self, cx: &mut Context<Self>) {
        self.idle_memory.activity = Instant::now();
        if self.idle_memory.task.is_some() {
            return;
        }
        self.idle_memory.task = Some(cx.spawn(async move |view, cx| {
            loop {
                cx.background_executor().timer(SETTLE).await;
                let Ok(idle) = view.update(cx, |view, _| {
                    if view.loading
                        || view.busy
                        || view.queue_running
                        || view.directory_sizes.is_some()
                        || view.idle_memory.activity.elapsed() < SETTLE
                    {
                        return false;
                    }
                    view.idle_memory.task = None;
                    true
                }) else {
                    return;
                };
                if idle {
                    cx.background_executor()
                        .spawn(async { release_unused_pages() })
                        .detach();
                    return;
                }
            }
        }));
    }
}
