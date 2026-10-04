//! Application state and asynchronous filesystem operations, independent of layout.
use crate::{
    files::{Entry, read_directory},
    navigation::History,
};
use gpui::{
    Context, FocusHandle, KeyDownEvent, ScrollStrategy, Task, UniformListScrollHandle, Window,
};
use std::{path::PathBuf, process::Command};

pub struct FileManager {
    pub(crate) path: PathBuf,
    pub(crate) home: PathBuf,
    pub(crate) places: Vec<crate::places::Place>,
    pub(crate) entries: Vec<Entry>,
    pub(crate) history: History,
    pub(crate) hidden: bool,
    pub(crate) loading: bool,
    pub(crate) error: Option<String>,
    pub(crate) selected: Option<usize>,
    pub(crate) scroll: UniformListScrollHandle,
    pub(crate) focus: FocusHandle,
    listing: Option<Task<()>>,
}

impl FileManager {
    pub fn new(path: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        let focus = cx.focus_handle();
        focus.focus(window);
        let mut view = Self {
            history: History::new(path.clone()),
            path: path.clone(),
            places: crate::places::discover(&home),
            home,
            entries: Vec::new(),
            hidden: false,
            loading: false,
            error: None,
            selected: None,
            scroll: UniformListScrollHandle::new(),
            focus,
            listing: None,
        };
        view.navigate(path, cx);
        view
    }

    pub(crate) fn navigate(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.load(path, None, cx);
    }

    fn load(&mut self, path: PathBuf, history_index: Option<usize>, cx: &mut Context<Self>) {
        self.loading = true;
        self.error = None;
        let hidden = self.hidden;
        let requested = path.clone();
        let read = cx
            .background_executor()
            .spawn(async move { read_directory(&requested, hidden) });
        // Replacing this task cancels the UI update from an outdated request.
        self.listing = Some(cx.spawn(async move |view, cx| {
            let result = read.await;
            let _ = view.update(cx, |view, cx| {
                view.loading = false;
                match result {
                    Ok(entries) => {
                        let selected_path = view
                            .selected
                            .and_then(|index| view.entries.get(index))
                            .map(|entry| entry.path.clone());
                        let changed = view.path != path;
                        if changed {
                            view.scroll = UniformListScrollHandle::new();
                        }
                        view.selected = if changed {
                            None
                        } else {
                            entries
                                .iter()
                                .position(|entry| Some(&entry.path) == selected_path.as_ref())
                        };
                        if let Some(index) = history_index {
                            view.history.restore(index);
                        } else {
                            view.history.visit(path.clone());
                        }
                        view.path = path;
                        view.entries = entries;
                    }
                    Err(error) => {
                        view.error = Some(format!("Cannot read {}: {error}", path.display()))
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(crate) fn back(&mut self, cx: &mut Context<Self>) {
        if let Some((index, path)) = self.history.back() {
            self.load(path, Some(index), cx);
        }
    }

    pub(crate) fn forward(&mut self, cx: &mut Context<Self>) {
        if let Some((index, path)) = self.history.forward() {
            self.load(path, Some(index), cx);
        }
    }

    pub(crate) fn up(&mut self, cx: &mut Context<Self>) {
        if let Some(parent) = self.path.parent() {
            self.navigate(parent.to_path_buf(), cx);
        }
    }

    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        self.navigate(self.path.clone(), cx);
    }

    pub(crate) fn toggle_hidden(&mut self, cx: &mut Context<Self>) {
        self.hidden = !self.hidden;
        self.refresh(cx);
    }

    pub(crate) fn open_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(entry) = self
            .selected
            .and_then(|index| self.entries.get(index))
            .cloned()
        {
            self.open(entry, cx);
        }
    }

    pub(crate) fn open(&mut self, entry: Entry, cx: &mut Context<Self>) {
        if entry.directory {
            self.navigate(entry.path, cx);
            return;
        }
        self.error = None;
        cx.notify();
        let open = cx.background_executor().spawn(async move {
            Command::new("xdg-open")
                .arg(&entry.path)
                .output()
                .map_err(|error| format!("Cannot open {}: {error}", entry.path.display()))
                .and_then(|output| {
                    if output.status.success() {
                        Ok(())
                    } else {
                        Err(format!(
                            "Cannot open {}: {}",
                            entry.path.display(),
                            String::from_utf8_lossy(&output.stderr).trim()
                        ))
                    }
                })
        });
        cx.spawn(async move |view, cx| {
            if let Err(error) = open.await {
                let _ = view.update(cx, |view, cx| {
                    view.error = Some(error);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub(crate) fn key_down(
        &mut self,
        event: &KeyDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        match key {
            "left" if modifiers.alt => self.back(cx),
            "right" if modifiers.alt => self.forward(cx),
            "up" if modifiers.alt => self.up(cx),
            "h" if modifiers.control => self.toggle_hidden(cx),
            "f5" => self.refresh(cx),
            "enter" if !modifiers.modified() => self.open_selected(cx),
            "escape" => {
                self.selected = None;
                cx.notify();
            }
            "up" | "down" if !modifiers.modified() && !self.entries.is_empty() => {
                let index = match (self.selected, key) {
                    (Some(index), "up") => index.saturating_sub(1),
                    (Some(index), _) => (index + 1).min(self.entries.len() - 1),
                    (None, "up") => self.entries.len() - 1,
                    (None, _) => 0,
                };
                self.selected = Some(index);
                self.scroll.scroll_to_item(index, ScrollStrategy::Center);
                cx.notify();
            }
            _ => {}
        }
    }
}
