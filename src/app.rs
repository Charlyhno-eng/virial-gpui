//! Application state and asynchronous filesystem operations, independent of layout.
use crate::{
    files::{Entry, read_directory},
    i18n::Language,
    location::Location,
    navigation::History,
};
use gpui::{
    Context, FocusHandle, KeyDownEvent, ScrollStrategy, Task, UniformListScrollHandle, Window,
};
use std::{path::PathBuf, process::Command};

pub struct FileManager {
    pub(crate) location: Location,
    pub(crate) language: Language,
    pub(crate) data_home: PathBuf,
    runtime_home: PathBuf,
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
    pub(crate) menu: Option<crate::actions::Menu>,
    pub(crate) dialog: Option<crate::actions::Dialog>,
    pub(crate) clipboard: Option<(PathBuf, bool)>,
    pub(crate) busy: bool,
    pub(crate) titlebar_drag: Option<gpui::Point<gpui::Pixels>>,
}

impl FileManager {
    pub fn new(path: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        let focus = cx.focus_handle();
        focus.focus(window);
        let mut view = Self {
            history: History::new(path.clone().into()),
            location: path.clone().into(),
            language: Language::system(),
            data_home: crate::recent::data_home(&home),
            runtime_home: crate::network::runtime_home(),
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
            menu: None,
            dialog: None,
            clipboard: None,
            busy: false,
            titlebar_drag: None,
        };
        view.navigate(path, cx);
        view
    }

    pub(crate) fn navigate(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.navigate_location(path.into(), cx);
    }

    pub(crate) fn navigate_location(&mut self, location: Location, cx: &mut Context<Self>) {
        self.load(location, None, cx);
    }

    fn load(&mut self, location: Location, history_index: Option<usize>, cx: &mut Context<Self>) {
        self.menu = None;
        self.loading = true;
        self.error = None;
        let hidden = self.hidden;
        let requested = location.clone();
        let data = self.data_home.clone();
        let runtime = self.runtime_home.clone();
        let read = cx.background_executor().spawn(async move {
            match requested {
                Location::Directory(path) => read_directory(&path, hidden),
                Location::Recent => crate::recent::read(&data, hidden),
                Location::Network => crate::network::read(&runtime),
            }
        });
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
                        let changed = view.location != location;
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
                            view.history.visit(location.clone());
                        }
                        view.location = location;
                        view.entries = entries;
                    }
                    Err(error) => {
                        view.error = Some(format!(
                            "{} {}: {error}",
                            view.language.text("Cannot read"),
                            location.description(view.language)
                        ))
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
        if let Some(parent) = self.location.directory().and_then(|path| path.parent()) {
            self.navigate(parent.to_path_buf(), cx);
        }
    }

    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        self.navigate_location(self.location.clone(), cx);
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
        let data = self.data_home.clone();
        let language = self.language;
        let opened_path = entry.path.clone();
        let open = cx.background_executor().spawn(async move {
            Command::new("xdg-open")
                .arg(&entry.path)
                .output()
                .map_err(|error| {
                    format!(
                        "{} {}: {error}",
                        language.text("Cannot open"),
                        entry.path.display()
                    )
                })
                .and_then(|output| {
                    if output.status.success() {
                        crate::recent::record(&data, &opened_path).map_err(|error| {
                            format!("{}: {error}", language.text("Cannot save recent history"))
                        })
                    } else {
                        Err(format!(
                            "{} {}: {}",
                            language.text("Cannot open"),
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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.action_key(event, window, cx) {
            return;
        }
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
