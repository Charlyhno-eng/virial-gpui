use crate::{
    app::FileManager,
    infrastructure::search::{SearchResults, search},
    state::app_state::DirectorySizeTask,
};
use gpui::{Context, KeyDownEvent, Window};
use std::{
    sync::{Arc, atomic::AtomicBool, mpsc},
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
        let picker = self.global_search.get_or_insert_with(|| GlobalSearch {
            query: String::new(),
            results: SearchResults::default(),
            selected: 0,
            selection_moved: false,
            pending_open: false,
            task: None,
            scroll: gpui::UniformListScrollHandle::new(),
        });
        if query == picker.query {
            return;
        }
        picker.task = None;
        picker.query = query.clone();
        picker.results = SearchResults::default();
        picker.selected = 0;
        picker.selection_moved = false;
        picker.pending_open = false;
        picker.scroll = gpui::UniformListScrollHandle::new();
        cx.notify();
        let roots = vec![std::path::PathBuf::from("/"), self.home.clone()];
        let hidden = true;
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let executor = cx.background_executor().clone();
        let (sender, receiver) = mpsc::channel();
        let worker = executor.clone().spawn(async move {
            executor.timer(Duration::from_millis(200)).await;
            search(roots, &query, hidden, &worker_cancelled, |results| {
                sender.send(results).is_ok()
            });
        });
        let executor = cx.background_executor().clone();
        let task = cx.spawn(async move |view, cx| {
            loop {
                executor.timer(Duration::from_millis(100)).await;
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
        } else if self.global_search.is_none() {
            return false;
        } else if event.keystroke.key != "escape"
            && !self.global_search.as_ref().unwrap().pending_open
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
