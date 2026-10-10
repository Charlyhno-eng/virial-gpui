use crate::{
    app::FileManager, infrastructure::search::SearchResults, state::app_state::DirectorySizeTask,
};
use gpui::{Context, KeyDownEvent, Window};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

pub(crate) struct GlobalSearch {
    pub query: String,
    pub results: SearchResults,
    pub selected: usize,
    pub selection_moved: bool,
    pub pending_open: bool,
    pub task: Option<DirectorySizeTask>,
    pub scroll: gpui::UniformListScrollHandle,
    /// Whether the results panel is expanded from the search field.
    pub expanded: bool,
}

/// Where the "everywhere" index walks. Linux keeps the historical whole-tree
/// root; Windows indexes the user profile only — a literal "/" matches no
/// Windows path and indexing every drive would crawl C:\Windows each rescan.
fn search_roots(home: &std::path::Path) -> Vec<std::path::PathBuf> {
    #[cfg(unix)]
    {
        let _ = home;
        vec!["/".into()]
    }
    #[cfg(windows)]
    vec![home.to_path_buf()]
}

fn queued_result(picker: &GlobalSearch) -> Option<crate::domain::models::Entry> {
    let entry = picker.results.entries.get(picker.selected)?;
    (picker.pending_open && (picker.results.finished || entry.name.to_lowercase() == picker.query))
        .then(|| entry.clone())
}

impl FileManager {
    pub(crate) fn show_global_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            if self.dialog.is_some() {
                return;
            }
            self.search_input.read(cx).focus(window);
            self.update_global_search(cx);
            if let Some(picker) = self.global_search.as_mut() {
                picker.expanded = true;
            }
            cx.notify();
        }

        /// Fold or unfold the results panel from the search field's triangle,
        /// keeping the query and the ranking intact.
        pub(crate) fn toggle_global_search(&mut self, cx: &mut Context<Self>) {
            match self.global_search.as_mut() {
                Some(picker) => picker.expanded = !picker.expanded,
                None => {
                    self.update_global_search(cx);
                    if let Some(picker) = self.global_search.as_mut() {
                        picker.expanded = true;
                    }
                }
            }
            cx.notify();
        }

    fn clear_global_search(&mut self, cx: &mut Context<Self>) {
        self.global_search = None;
        self.search_input.update(cx, |input, cx| {
            input.clear();
            cx.notify();
        });
    }

    pub(crate) fn close_global_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_global_search(cx);
        self.focus.focus(window);
        cx.notify();
    }

    pub(crate) fn update_global_search(&mut self, cx: &mut Context<Self>) {
        let query = self.search_input.read(cx).text.trim().to_lowercase();
        if query.is_empty() {
            self.global_search = None;
            cx.notify();
            return;
        }
        // Inventory can be large; browsing alone needs neither its thread nor RAM.
        let index = self
            .search_index
            .get_or_insert_with(|| {
                let cache = std::env::var_os("XDG_CACHE_HOME")
                    .map(std::path::PathBuf::from)
                    .filter(|path| path.is_absolute())
                    .unwrap_or_else(|| self.home.join(".cache"))
                    .join("virial/search/catalog-v1.jsonl");
                crate::infrastructure::search::SearchIndex::start(search_roots(&self.home), cache)
            })
            .handle();
        let picker = self.global_search.get_or_insert_with(|| GlobalSearch {
                    query: String::new(),
                    results: SearchResults::default(),
                    selected: 0,
                    selection_moved: false,
                    pending_open: false,
                    task: None,
                    scroll: gpui::UniformListScrollHandle::new(),
                    expanded: false,
                });
        if query == picker.query {
                    return;
                }
                // A new query reopens the panel: folding it was an explicit user
                // gesture about the previous results, not about this one.
                picker.expanded = true;
                picker.task = None;
        picker.query = query.clone();
        picker.results = SearchResults::default();
        picker.selected = 0;
        picker.selection_moved = false;
        picker.pending_open = false;
        picker.scroll = gpui::UniformListScrollHandle::new();
        cx.notify();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let executor = cx.background_executor().clone();
        // Keep only one pending batch; a slow frame must not accumulate old results.
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker_query = query.clone();
        let worker = executor.clone().spawn(async move {
            // Coalesce keystrokes before scoring a potentially large catalog.
            executor.timer(Duration::from_millis(120)).await;
            let mut previous = None;
            loop {
                if worker_cancelled.load(Ordering::Relaxed) {
                    break;
                }
                let revision = index.revision();
                if previous != Some(revision) {
                    let Some(results) = index.query(&worker_query, true, &worker_cancelled) else {
                        break;
                    };
                    match sender.try_send(results) {
                        Ok(()) => previous = Some(revision),
                        Err(mpsc::TrySendError::Full(_)) => {}
                        Err(mpsc::TrySendError::Disconnected(_)) => break,
                    }
                }
                // Stream inventory changes without rescoring every notification batch.
                executor.timer(Duration::from_millis(150)).await;
            }
        });
        let executor = cx.background_executor().clone();
        let task = cx.spawn(async move |view, cx| {
            loop {
                executor.timer(Duration::from_millis(50)).await;
                let mut latest = None;
                let mut disconnected = false;
                loop {
                    match receiver.try_recv() {
                        Ok(results) => latest = Some(results),
                        Err(mpsc::TryRecvError::Empty) => break,
                        Err(mpsc::TryRecvError::Disconnected) => {
                            disconnected = true;
                            break;
                        }
                    }
                }
                if let Some(results) = latest {
                    let _ = view.update(cx, |view, cx| {
                        if let Some(picker) = &mut view.global_search {
                            if picker.query != query {
                                return;
                            }
                            let selected_path = picker
                                .selection_moved
                                .then(|| {
                                    picker
                                        .results
                                        .entries
                                        .get(picker.selected)
                                        .map(|entry| entry.path.clone())
                                })
                                .flatten();
                            picker.selected = results
                                .entries
                                .iter()
                                .position(|entry| Some(&entry.path) == selected_path.as_ref())
                                .unwrap_or(0);
                            picker.results = results;
                            if picker.results.finished && picker.results.entries.is_empty() {
                                picker.pending_open = false;
                            }
                            if let Some(entry) = queued_result(picker) {
                                view.search_return_focus = true;
                                view.clear_global_search(cx);
                                view.open(entry, cx);
                            }
                            cx.notify();
                        }
                    });
                }
                if disconnected {
                    break;
                }
            }
            worker.await;
        });
        self.global_search.as_mut().unwrap().task = Some(DirectorySizeTask {
            _task: task,
            cancelled,
        });
    }

    pub(crate) fn open_global_result(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entry = self
            .global_search
            .as_ref()
            .and_then(|picker| picker.results.entries.get(index))
            .cloned();
        if let Some(entry) = entry {
            self.close_global_search(window, cx);
            self.open(entry, cx);
        }
    }

    pub(crate) fn global_search_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.key == "p" && event.keystroke.modifiers.control {
            self.show_global_search(window, cx);
            cx.stop_propagation();
            return true;
        }
        // Read the current text synchronously: Enter can precede the observer callback.
        if self.search_input.read(cx).is_focused(window) {
            self.update_global_search(cx);
            if self.global_search.is_none() {
                if event.keystroke.key == "enter" {
                    cx.stop_propagation();
                    return true;
                }
                return false;
            }
        } else if self.global_search.is_none()
            || (event.keystroke.key != "escape"
                && !self.global_search.as_ref().unwrap().pending_open)
        {
            return false;
        }
        match event.keystroke.key.as_str() {
            "escape" => self.close_global_search(window, cx),
            "enter" => {
                let index = self.global_search.as_ref().unwrap().selected;
                if self
                    .global_search
                    .as_ref()
                    .unwrap()
                    .results
                    .entries
                    .is_empty()
                {
                    let picker = self.global_search.as_mut().unwrap();
                    picker.pending_open = !picker.results.finished;
                    cx.notify();
                } else {
                    self.open_global_result(index, window, cx);
                }
            }
            "up" | "down" => {
                let picker = self.global_search.as_mut().unwrap();
                if picker.results.entries.is_empty() {
                    cx.stop_propagation();
                    return true;
                }
                picker.selection_moved = true;
                let last = picker.results.entries.len() - 1;
                picker.selected = if event.keystroke.key == "up" {
                    picker.selected.saturating_sub(1)
                } else {
                    (picker.selected + 1).min(last)
                };
                picker
                    .scroll
                    .scroll_to_item(picker.selected, gpui::ScrollStrategy::Center);
                cx.notify();
            }
            _ => return true,
        }
        cx.stop_propagation();
        true
    }
}

#[cfg(test)]
#[path = "../../tests/state/global_search.rs"]
mod tests;
