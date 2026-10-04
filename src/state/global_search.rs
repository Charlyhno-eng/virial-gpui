use crate::{
    app::FileManager,
    infrastructure::search::{SearchResults, search},
    state::app_state::DirectorySizeTask,
    ui::components::input::NameInput,
};
use gpui::{AppContext, Context, Entity, KeyDownEvent, Window};
use std::{
    sync::{Arc, atomic::AtomicBool, mpsc},
    time::Duration,
};

pub(crate) struct GlobalSearch {
    pub input: Entity<NameInput>,
    pub query: String,
    pub results: SearchResults,
    pub selected: usize,
    pub selection_moved: bool,
    pub task: Option<DirectorySizeTask>,
    pub scroll: gpui::UniformListScrollHandle,
}

impl FileManager {
    pub(crate) fn show_global_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog.is_some() || self.global_search.is_some() {
            return;
        }
        let input = cx.new(|cx| NameInput::new(String::new(), window, cx));
        cx.observe(&input, |view, _, cx| view.update_global_search(cx))
            .detach();
        self.menu = None;
        self.marquee = None;
        self.global_search = Some(GlobalSearch {
            input,
            query: String::new(),
            results: SearchResults::default(),
            selected: 0,
            selection_moved: false,
            task: None,
            scroll: gpui::UniformListScrollHandle::new(),
        });
        cx.notify();
    }

    pub(crate) fn close_global_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.global_search = None;
        self.focus.focus(window);
        cx.notify();
    }

    fn update_global_search(&mut self, cx: &mut Context<Self>) {
        let Some(picker) = self.global_search.as_mut() else {
            return;
        };
        let query = picker.input.read(cx).text.trim().to_lowercase();
        if query == picker.query {
            return;
        }
        picker.task = None;
        picker.query = query.clone();
        picker.results = SearchResults::default();
        picker.selected = 0;
        picker.selection_moved = false;
        picker.scroll = gpui::UniformListScrollHandle::new();
        cx.notify();
        if query.is_empty() {
            return;
        }
        let roots = vec![std::path::PathBuf::from("/"), self.home.clone()];
        let hidden = self.hidden;
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
        if self.global_search.is_none() {
            if event.keystroke.key == "p" && event.keystroke.modifiers.control {
                self.show_global_search(window, cx);
                cx.stop_propagation();
                return true;
            }
            return false;
        }
        match event.keystroke.key.as_str() {
            "escape" => self.close_global_search(window, cx),
            "enter" => {
                let index = self.global_search.as_ref().unwrap().selected;
                self.open_global_result(index, window, cx);
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
