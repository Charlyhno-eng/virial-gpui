//! Application orchestration and asynchronous filesystem work.
use crate::domain::models::Entry;
use crate::{
    domain::location::Location,
    domain::services::History,
    infrastructure::storage::{directory_size, read_directory},
    state::app_state::DirectorySizeTask,
    ui::i18n::Language,
};
use gpui::{AppContext, Context, KeyDownEvent, ScrollStrategy, UniformListScrollHandle, Window};
use std::{
    path::PathBuf,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, TryRecvError},
    },
    time::Duration,
};

pub(crate) use crate::state::app_state::FileManager;

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
            data_home: crate::infrastructure::recent::data_home(&home),
            runtime_home: crate::platform::linux::network::runtime_home(),
            places: crate::platform::linux::places::discover(&home),
            home,
            entries: Vec::new(),
            folder_count: 0,
            hidden: false,
            loading: false,
            error: None,
            selection: Default::default(),
            marquee: None,
            external_drop: None,
            scroll: UniformListScrollHandle::new(),
            focus,
            listing: None,
            directory_sizes: None,
            menu: None,
            dialog: None,
            clipboard: None,
            busy: false,
            search_input: cx.new(|cx| {
                crate::ui::components::input::NameInput::new_unfocused(String::new(), cx)
            }),
            details_open: true,
            compact_view: false,
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
        self.directory_sizes = None;
        self.menu = None;
        self.marquee = None;
        self.loading = true;
        self.error = None;
        let hidden = self.hidden;
        let requested = location.clone();
        let data = self.data_home.clone();
        let runtime = self.runtime_home.clone();
        let read = cx.background_executor().spawn(async move {
            match requested {
                Location::Directory(path) => read_directory(&path, hidden),
                Location::Recent => crate::infrastructure::recent::read(&data, hidden),
                Location::Network => crate::platform::linux::network::read(&runtime),
            }
        });
        // Replacing this task cancels the UI update from an outdated request.
        self.listing = Some(cx.spawn(async move |view, cx| {
            let result = read.await;
            let _ = view.update(cx, |view, cx| {
                view.loading = false;
                match result {
                    Ok(entries) => {
                        let selected_paths: std::collections::HashSet<_> =
                            view.selected_paths().into_iter().collect();
                        let focus_path = view
                            .selection
                            .focus
                            .and_then(|index| view.entries.get(index))
                            .map(|entry| entry.path.clone());
                        let anchor_path = view
                            .selection
                            .anchor
                            .and_then(|index| view.entries.get(index))
                            .map(|entry| entry.path.clone());
                        let changed = view.location != location;
                        view.marquee = None;
                        view.selection.clear();
                        if changed {
                            view.scroll = UniformListScrollHandle::new();
                        } else {
                            view.selection.indices = entries
                                .iter()
                                .enumerate()
                                .filter(|(_, entry)| selected_paths.contains(&entry.path))
                                .map(|(index, _)| index)
                                .collect();
                            view.selection.focus = entries
                                .iter()
                                .position(|entry| Some(&entry.path) == focus_path.as_ref());
                            view.selection.anchor = entries
                                .iter()
                                .position(|entry| Some(&entry.path) == anchor_path.as_ref());
                        }
                        if let Some(index) = history_index {
                            view.history.restore(index);
                        } else {
                            view.history.visit(location.clone());
                        }
                        view.location = location;
                        view.folder_count = entries.iter().filter(|entry| entry.directory).count();
                        view.entries = entries;
                        view.load_directory_sizes(cx);
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

    fn load_directory_sizes(&mut self, cx: &mut Context<Self>) {
        let folders = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.directory)
            .map(|(index, entry)| (index, entry.path.clone()))
            .collect::<Vec<_>>();
        if folders.is_empty() {
            return;
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let executor = cx.background_executor().clone();
        let (sender, receiver) = mpsc::channel();
        // One worker limits I/O contention. Deliver each result separately so a
        // large folder cannot hold up sizes that have already been calculated.
        let read = executor.spawn(async move {
            for (index, path) in folders {
                if worker_cancelled.load(Ordering::Relaxed) {
                    break;
                }
                if let Ok(bytes) = directory_size(&path, &worker_cancelled)
                    && sender.send((index, bytes)).is_err()
                {
                    break;
                }
            }
        });
        let task = cx.spawn(async move |view, cx| {
            loop {
                // Redraw at most ten times a second, independently of scan speed.
                executor.timer(Duration::from_millis(100)).await;
                let mut sizes = Vec::new();
                let mut finished = false;
                loop {
                    match receiver.try_recv() {
                        Ok(size) => sizes.push(size),
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => {
                            finished = true;
                            break;
                        }
                    }
                }
                if !sizes.is_empty()
                    && view
                        .update(cx, |view, cx| {
                            for (index, bytes) in sizes {
                                view.entries[index].bytes = Some(bytes);
                            }
                            cx.notify();
                        })
                        .is_err()
                {
                    return;
                }
                if finished {
                    break;
                }
            }
            read.await;
        });
        self.directory_sizes = Some(DirectorySizeTask {
            _task: task,
            cancelled,
        });
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
            .selection
            .primary()
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
                        crate::infrastructure::recent::record(&data, &opened_path).map_err(
                            |error| {
                                format!("{}: {error}", language.text("Cannot save recent history"))
                            },
                        )
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
        if event.keystroke.key == "f11" && !event.keystroke.modifiers.modified() {
            self.titlebar_drag = None;
            window.toggle_fullscreen();
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if self.action_key(event, window, cx) {
            return;
        }
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        if key == "escape" && cx.stop_active_drag(window) {
            self.external_drop = None;
            cx.notify();
            return;
        }
        match key {
            "left" if modifiers.alt => self.back(cx),
            "right" if modifiers.alt => self.forward(cx),
            "up" if modifiers.alt => self.up(cx),
            "h" if modifiers.control => self.toggle_hidden(cx),
            "f5" if !modifiers.modified() => self.refresh(cx),
            "left" | "backspace" if !modifiers.modified() => self.up(cx),
            "right" if !modifiers.modified() => {
                if let Some(entry) = self
                    .selection
                    .primary()
                    .and_then(|index| self.entries.get(index))
                    .filter(|entry| entry.directory)
                    .cloned()
                {
                    self.open(entry, cx);
                }
            }
            "enter" if !modifiers.modified() => self.open_selected(cx),
            "escape" => {
                if window.is_fullscreen() {
                    window.toggle_fullscreen();
                }
                self.selection.clear();
                self.marquee = None;
                cx.notify();
            }
            "up" | "down" | "home" | "end" | "pageup" | "pagedown"
                if !modifiers.control
                    && !modifiers.alt
                    && !modifiers.platform
                    && !self.entries.is_empty() =>
            {
                let height = f32::from(self.scroll.0.borrow().base_handle.bounds().size.height);
                let page = (height / crate::ui::theme::ROW_HEIGHT).floor() as usize;
                let Some(index) = self
                    .selection
                    .keyboard_target(key, self.entries.len(), page)
                else {
                    return;
                };
                self.selection.click(index, false, modifiers.shift);
                self.scroll.scroll_to_item(index, ScrollStrategy::Center);
                cx.notify();
            }
            _ => return,
        }
        cx.stop_propagation();
    }
}
