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
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

pub(crate) use crate::state::app_state::FileManager;

impl FileManager {
    pub fn new(path: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new_pane(path, true, window, cx)
    }

    pub(crate) fn new_pane(
        path: PathBuf,
        recover: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // The portable chain (HOME, USERPROFILE, HOMEDRIVE+HOMEPATH) instead of
        // a bare HOME: a double-clicked exe has no POSIX environment, and the
        // old "/" fallback scattered app data across the drive root.
        let home = crate::config::home();
        let data_home = crate::infrastructure::recent::data_home(&home);
        Self::with_paths(path, home, data_home, recover, window, cx)
    }

    pub(crate) fn with_paths(
        path: PathBuf,
        home: PathBuf,
        data_home: PathBuf,
        recover: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window);
        let mut view = Self {
            embedded: true,
            compact: false,
            view_mode: Default::default(),
            grid_columns: 1,
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            idle_memory: Default::default(),
            history: History::new(path.clone().into()),
            location: path.clone().into(),
            language: Language::system(),
            data_home: data_home.clone(),
            places: crate::platform::places::discover(&home),
            devices: Vec::new(),
            device_error: None,
            device_monitor: None,
            device_generation: 0,
            home,
            entries: Vec::new(),
            entry_modified: Default::default(),
            workspaces: Vec::new(),
            tags: Default::default(),
            folder_count: 0,
            loading: false,
            navigation_generation: 0,
            drop_hover: None,
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
            rename: None,
            clipboard: Default::default(),
            busy: false,
            transfer_progress: None,
            operation_queue: Default::default(),
            active_operation: None,
            queue_running: false,
            queue_failed: false,
            queue_background: false,
            filter_open: false,
            search_open: false,
            extension_filter: String::new(),
            extension_input: cx.new(|cx| {
                let mut input =
                    crate::ui::components::input::NameInput::new_unfocused(String::new(), cx);
                input.compact = true;
                input.placeholder = Language::system()
                    .text("Filter by name or extension…")
                    .into();
                input
            }),
            search_input: cx.new(|cx| {
                let mut input =
                    crate::ui::components::input::NameInput::new_unfocused(String::new(), cx);
                input.compact = true;
                input.search_icon = true;
                input.placeholder = Language::system().text("Search everywhere…").into();
                input
            }),
            global_search: None,
            search_index: None,
            search_return_focus: false,
            details_open: false,
            sidebar_width: crate::ui::theme::SIDEBAR_WIDTH,
            sidebar_transition: None,
            preview_expanded: false,
            preview_generation: 0,
            preview_root: None,
            preview_entry: None,
            preview_history: Vec::new(),
            preview_active_column: 0,
            preview_selection: Default::default(),
            preview_columns_scroll: gpui::ScrollHandle::new(),
            preview_column_widths: Vec::new(),
            layout: crate::infrastructure::layout::Layout::load(),
            preview_resize: None,
            preview_path: None,
            preview: crate::state::preview::Preview::Unavailable,
            preview_line: 0,
            preview_scroll: gpui::ScrollHandle::new(),
            code_preview_scroll: gpui::UniformListScrollHandle::new(),
            preview_focused: false,
            editor: None,
            editor_disk: None,
            editor_task: None,
            preview_task: None,
            pending_preview: None,
            preview_media_image: None,
            preview_image_cache: gpui::RetainAllImageCache::new(cx),
            preview_media_updates: None,
            opened_archive_files: Vec::new(),
            remote_cache_files: Vec::new(),
            name_descending: false,
            titlebar_drag: None,
            ssh: crate::state::ssh::SshManager::new(&data_home),
        };
        view.layout.capture(window);
        cx.observe_window_bounds(window, |view, window, _| {
            view.layout.capture(window);
        })
        .detach();
        cx.on_release(|view, _| {
            if !cfg!(test) {
                view.layout.save();
            }
            for cached in view.remote_cache_files.drain(..) {
                let _ = std::fs::remove_file(cached);
            }
        })
        .detach();
        cx.on_app_quit(|view, _| {
            if !cfg!(test) {
                view.layout.save();
            }
            // Tear the SSH sessions down and drop the remote-file cache.
            view.ssh.shutdown();
            for cached in view.remote_cache_files.drain(..) {
                let _ = std::fs::remove_file(cached);
            }
            async {}
        })
        .detach();
        cx.observe(&view.search_input, |view, _, cx| {
            view.update_global_search(cx)
        })
        .detach();
        cx.observe(&view.extension_input, |view, _, cx| {
            let extension = view
                .extension_input
                .read(cx)
                .text
                .trim()
                .trim_start_matches('.')
                .to_lowercase();
            if extension != view.extension_filter {
                view.extension_filter = extension;
                view.refresh(cx);
            }
        })
        .detach();
        view.navigate(path, cx);
        view.monitor_devices(cx);
        if recover {
            view.recover_operations(cx);
        }
        view
    }

    pub(crate) fn navigate(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.navigate_location(path.into(), cx);
    }

    pub(crate) fn navigate_location(&mut self, location: Location, cx: &mut Context<Self>) {
        self.load(location, None, cx);
    }

    fn load(&mut self, location: Location, history_index: Option<usize>, cx: &mut Context<Self>) {
        self.cancel_pending_preview();
        self.preview = crate::state::preview::Preview::Unavailable;
        self.preview_task = None;
        self.preview_media_updates = None;
        self.preview_image_cache = gpui::RetainAllImageCache::new(cx);
        self.preview_path = None;
        self.preview_root = None;
        self.preview_entry = None;
        self.preview_history.clear();
        self.preview_active_column = 0;
        self.preview_selection.clear();
        self.preview_expanded = false;
        self.details_open = false;
        self.directory_sizes = None;
        self.menu = None;
        self.rename = None;
        self.marquee = None;
        self.loading = true;
        self.navigation_generation = self.navigation_generation.wrapping_add(1);
        self.error = None;
        let hidden = true;
        let requested = location.clone();
        let data = self.data_home.clone();
        let descending = self.name_descending;
        let ssh_store = self.ssh.store.clone();
        let ssh_runtime = self.ssh.runtime.handle().clone();
        let local_metadata = !matches!(requested, Location::Remote { .. });
        let read = cx.background_executor().spawn(async move {
            let tags = crate::infrastructure::tags::read(&data);
            let listed: std::io::Result<Vec<Entry>> = match requested {
                Location::Tag(tag) => tags
                    .as_ref()
                    .map_err(|error| std::io::Error::other(error.to_string()))?
                    .entries(tag),
                Location::Directory(path) => read_directory(&path, hidden),
                Location::Remote { host, path } => {
                    // Browsing a connected host: the SFTP listing replaces
                    // the local directory read; failures surface as errors.
                    ssh_store
                        .browse(&host, &path, Some(ssh_runtime))
                        .await
                        .map(|mut entries| {
                            crate::state::browser::sort_by_name(&mut entries);
                            entries
                        })
                        .map_err(std::io::Error::other)
                }
                Location::Recent => {
                    crate::infrastructure::recent::read(&data, hidden).map(|mut entries| {
                        crate::state::browser::sort_by_name(&mut entries);
                        entries
                    })
                }
                Location::Trash => crate::infrastructure::trash::read(&data),
                Location::Workspaces => {
                    return crate::infrastructure::workspaces::summaries(&data, hidden)
                        .map(|summaries| (Vec::new(), summaries, Default::default(), tags));
                }
            };
            listed.map(|mut entries: Vec<Entry>| {
                // Directory, ZIP and Trash readers already sort by name.
                // Apply the direction on the worker before restoring selection.
                crate::state::browser::apply_name_direction(&mut entries, descending);
                let modified: std::collections::HashMap<_, _> = if local_metadata {
                    entries
                        .iter()
                        .filter_map(|entry| {
                            let modified = std::fs::metadata(&entry.path).ok()?.modified().ok()?;
                            Some((
                                entry.path.clone(),
                                crate::state::browser::Modified::Local(modified),
                            ))
                        })
                        .collect()
                } else {
                    Default::default()
                };
                (entries, Vec::new(), modified, tags)
            })
        });
        // Replacing this task cancels the UI update from an outdated request.
        let executor = cx.background_executor().clone();
        let transition = executor.timer(Duration::from_millis(65));
        self.listing = Some(cx.spawn(async move |view, cx| {
            let result = read.await;
            transition.await;
            let _ = view.update(cx, |view, cx| {
                view.loading = false;
                match result {
                    Ok((mut entries, workspaces, mut modified, tags)) => {
                        match tags {
                            Ok(tags) => view.tags = tags,
                            Err(error) => {
                                view.error = Some(format!(
                                    "{}: {error}",
                                    view.language.text("Cannot read tags")
                                ))
                            }
                        }
                        if descending != view.name_descending {
                            crate::state::browser::apply_name_direction(&mut entries, true);
                        }
                        let unfiltered_count = entries.len();
                        entries.retain(|entry| {
                            if local_metadata {
                                crate::state::browser::matches_file_filter(
                                    entry,
                                    &view.extension_filter,
                                )
                            } else {
                                crate::state::browser::matches_extension(
                                    entry,
                                    &view.extension_filter,
                                )
                            }
                        });
                        if entries.len() < unfiltered_count {
                            let visible_paths: std::collections::HashSet<_> =
                                entries.iter().map(|entry| &entry.path).collect();
                            modified.retain(|path, _| visible_paths.contains(path));
                            modified.shrink_to_fit();
                            entries.shrink_to_fit();
                        }
                        view.workspaces = workspaces;
                        let selected_paths: std::collections::HashSet<_> = view
                            .selection
                            .indices
                            .iter()
                            .filter_map(|index| view.entries.get(*index))
                            .map(|entry| &entry.path)
                            .collect();
                        let focus_path = view
                            .selection
                            .focus
                            .and_then(|index| view.entries.get(index))
                            .map(|entry| &entry.path);
                        let anchor_path = view
                            .selection
                            .anchor
                            .and_then(|index| view.entries.get(index))
                            .map(|entry| &entry.path);
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
                                .position(|entry| Some(&entry.path) == focus_path);
                            view.selection.anchor = entries
                                .iter()
                                .position(|entry| Some(&entry.path) == anchor_path);
                        }
                        if let Some(index) = history_index {
                            view.history.restore(index);
                        } else {
                            view.history.visit(location.clone());
                        }
                        view.location = location;
                        view.folder_count = entries.iter().filter(|entry| entry.directory).count();
                        view.entries = entries;
                        view.entry_modified = modified;
                        view.load_directory_sizes(cx);
                        view.preview_path = None;
                        view.preview_task = None;
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

    pub(crate) fn load_directory_sizes(&mut self, cx: &mut Context<Self>) {
        if self.directory_sizes.is_some()
            || self.busy
            || self.queue_running
            || matches!(
                self.location,
                Location::Remote { .. } | Location::Workspaces
            )
        {
            return;
        }
        let paths: Vec<_> = self
            .entries
            .iter()
            .filter(|entry| entry.directory && entry.bytes.is_none())
            .map(|entry| entry.path.clone())
            .collect();
        if paths.is_empty() {
            return;
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let generation = self.navigation_generation;
        let task = cx.spawn(async move |view, cx| {
            // One scan at a time, yielding between folders. Results appear
            // progressively, and navigating away cancels filesystem work.
            for path in paths {
                if worker_cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                let token = worker_cancelled.clone();
                let read = cx
                    .background_executor()
                    .spawn(async move { (directory_size(&path, &token), path) });
                let (result, path) = read.await;
                if let Ok(bytes) = result {
                    let _ = view.update(cx, |view, cx| {
                        if view.navigation_generation == generation
                            && let Some(entry) =
                                view.entries.iter_mut().find(|entry| entry.path == path)
                        {
                            entry.bytes = Some(bytes);
                            cx.notify();
                        }
                    });
                }
                cx.background_executor()
                    .timer(Duration::from_millis(30))
                    .await;
            }
            let _ = view.update(cx, |view, _| {
                if view.navigation_generation == generation {
                    view.directory_sizes = None;
                }
            });
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

    pub(crate) fn cancel_pending_preview(&mut self) {
        self.pending_preview = None;
        if let crate::state::preview::Preview::Media(media) = &self.preview {
            media.control(crate::infrastructure::media::Control::Pause(true));
        }
    }

    pub(crate) fn schedule_preview(
        &mut self,
        cx: &mut Context<Self>,
        preview: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
    ) {
        self.cancel_pending_preview();
        self.pending_preview = Some(cx.spawn(async move |view, cx| {
            // Match GPUI's Linux double-click interval. A second mouse-down
            // cancels this task before it can change the column layout.
            cx.background_executor()
                .timer(Duration::from_millis(400))
                .await;
            let _ = view.update(cx, |view, cx| {
                view.pending_preview = None;
                preview(view, cx);
            });
        }));
    }

    pub(crate) fn show_preview(&mut self, cx: &mut Context<Self>) {
        if self.view_mode != crate::state::navigation::ViewMode::List {
            return;
        }
        self.cancel_pending_preview();
        self.details_open = true;
        self.preview_active_column = 0;
        self.preview_focused = false;
        self.sync_preview(cx);
        cx.notify();
    }

    pub(crate) fn open(&mut self, entry: Entry, cx: &mut Context<Self>) {
        self.cancel_pending_preview();
        if self.location == Location::Trash {
            return;
        }
        if entry.browsable() {
            // A remote directory opens by navigating; a remote file needs a
            // local materialized copy first.
            if let Location::Remote { host, .. } = &self.location {
                if entry.directory {
                    let host = host.clone();
                    let path = entry.path;
                    self.navigate_location(Location::Remote { host, path }, cx);
                } else {
                    self.open_remote_file(host.clone(), entry, cx);
                }
                return;
            }
            self.navigate(entry.path, cx);
            return;
        }
        self.error = None;
        cx.notify();
        let data = self.data_home.clone();
        let language = self.language;
        let opened_path = entry.path.clone();
        let open = cx.background_executor().spawn(async move {
            let extracted = if crate::infrastructure::archive::is_member(&entry.path) {
                Some(
                    crate::infrastructure::archive::materialize(&entry.path, u64::MAX)
                        .map_err(|error| format!("{}: {error}", language.text("Cannot open")))?,
                )
            } else {
                None
            };
            let path = extracted
                .as_ref()
                .map(|file| file.path.as_path())
                .unwrap_or(&entry.path);
            let result = crate::platform::shell_open::open(path)
                .map_err(|error| {
                    format!(
                        "{} {}: {error}",
                        language.text("Cannot open"),
                        entry.path.display()
                    )
                })
                .and_then(|()| {
                    crate::infrastructure::recent::record(&data, &opened_path).map_err(|error| {
                        format!("{}: {error}", language.text("Cannot save recent history"))
                    })
                });
            Ok::<_, String>((result, extracted))
        });
        cx.spawn(async move |view, cx| {
            let result = open.await;
            let _ = view.update(cx, |view, cx| {
                match result {
                    Ok((result, extracted)) => {
                        if let Some(extracted) = extracted {
                            view.opened_archive_files.push(extracted);
                        }
                        if let Err(error) = result {
                            view.error = Some(error);
                        }
                    }
                    Err(error) => view.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pending_preview = None;
        if event.keystroke.key == "f11" && !event.keystroke.modifiers.modified() {
            self.titlebar_drag = None;
            window.toggle_fullscreen();
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if self.rename.is_some() {
            match event.keystroke.key.as_str() {
                "escape" => {
                    self.cancel_rename(window, cx);
                    cx.stop_propagation();
                }
                "enter" if !event.keystroke.modifiers.modified() => {
                    self.confirm_rename(window, cx);
                    cx.stop_propagation();
                }
                _ => {}
            }
            // Unhandled keys must reach GPUI's text input handler for typing and IME.
            return;
        }
        // A focused editor owns the keyboard; save and escape are handled here
        // because they touch the session, not the buffer.
        if let Some(editor) = self.editor.clone()
            && editor.read(cx).is_focused(window)
        {
            let save = (event.keystroke.key == "s"
                && (event.keystroke.modifiers.control || event.keystroke.modifiers.platform))
                || event.keystroke.key == "f2";
            if save {
                self.save_editor(cx);
                cx.stop_propagation();
                return;
            }
            if event.keystroke.key == "escape" {
                self.close_editor(window, cx);
                cx.stop_propagation();
                return;
            }
            if event.keystroke.key == "f11" {
                return;
            }
        }
        if self.global_search_key(event, window, cx) {
            return;
        }
        if self.dialog.is_none()
            && event.keystroke.key == "e"
            && event.keystroke.modifiers.control
            && self.details_open
            && let Some(path) = self.preview_path.clone()
        {
            self.open_editor(path, window, cx);
            cx.stop_propagation();
            return;
        }
        if self.dialog.is_none()
            && event.keystroke.key == "w"
            && event.keystroke.modifiers.control
            && !event.keystroke.modifiers.alt
            && event.keystroke.modifiers.shift
        {
            self.focus.focus(window);
            if self.location == Location::Workspaces {
                self.back(cx);
            } else {
                self.navigate_location(Location::Workspaces, cx);
            }
            cx.stop_propagation();
            return;
        }
        if self.extension_input.read(cx).is_focused(window) {
            if event.keystroke.key == "escape" {
                self.filter_open = false;
                cx.notify();
            }
            if matches!(event.keystroke.key.as_str(), "enter" | "escape") {
                self.focus.focus(window);
                cx.stop_propagation();
            }
            return;
        }
        if self.action_key(event, window, cx) {
            return;
        }
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        if self.preview_focused
            && matches!(
                self.preview,
                crate::state::preview::Preview::Text(_)
                    | crate::state::preview::Preview::Code(_)
                    | crate::state::preview::Preview::Markdown(..)
                    | crate::state::preview::Preview::Html(..)
            )
            && !modifiers.modified()
            && matches!(key, "up" | "down" | "home" | "end" | "pageup" | "pagedown")
        {
            let line_count = self.preview.line_count();
            let viewport = if self.preview_is_source_list() {
                self.code_preview_scroll.0.borrow().base_handle.bounds()
            } else {
                self.preview_scroll.bounds()
            };
            let page = (f32::from(viewport.size.height) / 18.).floor() as usize;
            self.preview_line = match key {
                "up" => self.preview_line.saturating_sub(1),
                "down" => (self.preview_line + 1).min(line_count - 1),
                "home" => 0,
                "end" => line_count - 1,
                "pageup" => self.preview_line.saturating_sub(page.max(1)),
                "pagedown" => (self.preview_line + page.max(1)).min(line_count - 1),
                _ => self.preview_line,
            };
            self.scroll_preview_to_line();
            cx.notify();
            cx.stop_propagation();
            return;
        }
        if key == "escape" && cx.stop_active_drag(window) {
            self.external_drop = None;
            cx.notify();
            return;
        }
        if self.view_mode == crate::state::navigation::ViewMode::Grid
            && self.preview_active_column == 0
            && !self.preview_focused
            && !modifiers.control
            && !modifiers.alt
            && !modifiers.platform
            && matches!(
                key,
                "left" | "right" | "up" | "down" | "home" | "end" | "pageup" | "pagedown"
            )
        {
            let height = f32::from(self.scroll.0.borrow().base_handle.bounds().size.height);
            if let Some(index) = crate::state::navigation::grid_target(
                key,
                self.selection.focus,
                self.entries.len(),
                self.grid_columns,
                (height / self.row_height()) as usize,
            ) {
                self.selection.click(index, false, modifiers.shift);
                self.scroll
                    .scroll_to_item(self.scroll_index(index), ScrollStrategy::Center);
                cx.notify();
            }
            cx.stop_propagation();
            return;
        }
        match key {
            "left" if modifiers.alt => self.back(cx),
            "right" if modifiers.alt => self.forward(cx),
            "up" if modifiers.alt => self.up(cx),
            "f5" if !modifiers.modified() => self.refresh(cx),
            "left" if !modifiers.modified() => self.preview_keyboard_left(cx),
            "backspace" if !modifiers.modified() => self.up(cx),
            "right" if !modifiers.modified() => self.preview_keyboard_right(cx),
            "space"
                if !modifiers.modified()
                    && self.focus.is_focused(window)
                    && self.selection.primary().is_some()
                    && !self.loading =>
            {
                self.details_open = true;
                self.sync_preview(cx);
                self.preview_expanded = !self.preview_expanded;
                cx.notify();
            }
            "enter" if !modifiers.modified() => self.open_selected(cx),
            "escape" if self.location == Location::Workspaces => self.back(cx),
            "escape" => {
                if self.details_open || self.preview_expanded {
                    self.close_preview(cx);
                    cx.stop_propagation();
                    return;
                }
                if window.is_fullscreen() {
                    window.toggle_fullscreen();
                }
                self.selection.clear();
                self.marquee = None;
                cx.notify();
            }
            "up" | "down" | "home" | "end" | "pageup" | "pagedown"
                if !modifiers.control && !modifiers.alt && !modifiers.platform =>
            {
                if self.preview_keyboard_select(key, cx) {
                    cx.stop_propagation();
                    return;
                }
                let height = f32::from(self.scroll.0.borrow().base_handle.bounds().size.height);
                let page = (height / self.row_height()).floor() as usize;
                let Some(index) = self
                    .selection
                    .keyboard_target(key, self.entries.len(), page)
                else {
                    return;
                };
                self.selection.click(index, false, modifiers.shift);
                self.scroll
                    .scroll_to_item(self.scroll_index(index), ScrollStrategy::Center);
                if !modifiers.shift {
                    self.preview_active_column = 0;
                    self.preview_focused = false;
                    self.show_preview(cx);
                }
                cx.notify();
            }
            _ => return,
        }
        cx.stop_propagation();
    }

    /// Open a file living on a connected SSH host: download it into a local
    /// cache directory, then hand the local copy to the host's launcher. The
    /// cache is wiped on application exit.
    pub(crate) fn open_remote_file(
        &mut self,
        host: crate::infrastructure::ssh::HostId,
        entry: Entry,
        cx: &mut Context<Self>,
    ) {
        self.error = None;
        cx.notify();
        let store = self.ssh.store.clone();
        let runtime = self.ssh.runtime.clone();
        let language = self.language;
        let data = self.data_home.clone();
        let entry_name = entry.name.clone();
        let open = cx.background_executor().spawn(async move {
            // russh futures must be polled inside a tokio runtime.
            let downloaded = runtime
                .spawn(async move {
                    let Some(session) = store.session(&host).await else {
                        return Err(language.text("SSH session is not connected").to_string());
                    };
                    let bytes = session.read_file(&entry.path).await?;
                    let cache = data.join("virial/remote-cache");
                    std::fs::create_dir_all(&cache)
                        .map_err(|error| format!("{}: {error}", language.text("Cannot open")))?;
                    let name = entry
                        .name
                        .chars()
                        .filter(|ch| !ch.is_control() && *ch != '/')
                        .collect::<String>();
                    let local = cache.join(format!(
                        "{}-{}",
                        std::process::id(),
                        if name.is_empty() { "file" } else { &name }
                    ));
                    std::fs::write(&local, &bytes)
                        .map_err(|error| format!("{}: {error}", language.text("Cannot open")))?;
                    Ok::<_, String>(local)
                })
                .await
                .unwrap_or_else(|join| Err(format!("SSH task failed: {join}")))?;
            let result = crate::platform::shell_open::open(&downloaded).map_err(|error| {
                format!("{} {}: {error}", language.text("Cannot open"), entry_name)
            });
            Ok::<_, String>((result, downloaded))
        });
        cx.spawn(async move |view, cx| {
            let result = open.await;
            let _ = view.update(cx, |view, cx| {
                match result {
                    Ok((result, local)) => {
                        view.remote_cache_files.push(local);
                        if let Err(error) = result {
                            view.error = Some(error);
                        }
                    }
                    Err(error) => view.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
