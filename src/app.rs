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
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

const PREVIEW_CLICK_DELAY: Duration = Duration::from_millis(400);

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
            places: crate::platform::linux::places::discover(&home),
            devices: Vec::new(),
            device_error: None,
            device_monitor: None,
            device_generation: 0,
            home,
            entries: Vec::new(),
            workspaces: Vec::new(),
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
            clipboard: None,
            busy: false,
            transfer_progress: None,
            operation_queue: Default::default(),
            active_operation: None,
            queue_running: false,
            queue_failed: false,
            queue_background: false,
            verify_transfers: false,
            extension_filter: String::new(),
            extension_input: cx.new(|cx| {
                let mut input =
                    crate::ui::components::input::NameInput::new_unfocused(String::new(), cx);
                input.compact = true;
                input.placeholder = Language::system().text("Extension (e.g. pdf)…").into();
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
            pending_preview: None,
            preview_click_generation: 0,
            layout: crate::infrastructure::layout::Layout::load(),
            preview_resize: None,
            preview_path: None,
            preview_modified: None,
            preview: crate::state::preview::Preview::Unavailable,
            preview_line: 0,
            preview_scroll: gpui::ScrollHandle::new(),
            preview_focused: false,
            preview_task: None,
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
        let weak = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            weak.update(cx, |view, cx| {
                if view.busy || view.queue_running {
                    view.error = Some(
                        view.language
                            .text("Wait for the current operation to finish before closing")
                            .into(),
                    );
                    cx.notify();
                    return false;
                }
                view.layout.capture(window);
                view.layout.save();
                true
            })
            .unwrap_or(true)
        });
        cx.on_release(|view, _| view.layout.save()).detach();
        cx.on_app_quit(|view, _| {
            view.layout.save();
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
        view.recover_operations(cx);
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
        self.preview_modified = None;
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
        let read = cx.background_executor().spawn(async move {
            match requested {
                Location::Directory(path) => read_directory(&path, hidden),
                Location::Remote { host, path } => {
                    // Browsing a connected host: the SFTP listing replaces the
                    // local directory read; failures surface as entry errors.
                    ssh_store
                        .browse(&host, &path, Some(ssh_runtime))
                        .await
                        .map(|mut entries| {
                            crate::state::browser::sort_by_name(&mut entries);
                            entries
                        })
                        .map_err(io::Error::other)
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
                        .map(|summaries| (Vec::new(), summaries));
                }
            }
            .map(|mut entries| {
                // Directory, ZIP and Trash readers already sort by name.
                // Apply the direction on the worker before restoring selection.
                crate::state::browser::apply_name_direction(&mut entries, descending);
                (entries, Vec::new())
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
                    Ok((mut entries, workspaces)) => {
                        if descending != view.name_descending {
                            crate::state::browser::apply_name_direction(&mut entries, true);
                        }
                        entries.retain(|entry| {
                            crate::state::browser::matches_extension(entry, &view.extension_filter)
                        });
                        view.workspaces = workspaces;
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
        let Some(entry) = self
            .selection
            .primary()
            .and_then(|index| self.entries.get(index))
            .filter(|entry| self.details_open && entry.directory && entry.bytes.is_none())
        else {
            return;
        };
        let path = entry.path.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        // Browsing Home or a drive must not recursively read every child tree.
        let read = cx
            .background_executor()
            .spawn(async move { (directory_size(&path, &worker_cancelled), path) });
        let task = cx.spawn(async move |view, cx| {
            let (result, path) = read.await;
            if let Ok(bytes) = result {
                let _ = view.update(cx, |view, cx| {
                    // Sorting may have changed row indices while the scan ran.
                    if let Some(entry) = view.entries.iter_mut().find(|entry| entry.path == path) {
                        entry.bytes = Some(bytes);
                        cx.notify();
                    }
                });
            }
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
        if let crate::state::preview::Preview::Media(media) = &self.preview {
            media.control(crate::infrastructure::media::Control::Pause(true));
        }
        self.preview_click_generation = self.preview_click_generation.wrapping_add(1);
        self.pending_preview = None;
    }

    pub(crate) fn defer_preview(&mut self, cx: &mut Context<Self>) {
        self.cancel_pending_preview();
        let generation = self.preview_click_generation;
        self.pending_preview = Some(cx.spawn(async move |view, cx| {
            cx.background_executor().timer(PREVIEW_CLICK_DELAY).await;
            let _ = view.update(cx, |view, cx| {
                if view.preview_click_generation == generation {
                    view.details_open = true;
                    cx.notify();
                }
            });
        }));
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
                if !entry.directory {
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
            let result = Command::new("xdg-open")
                .arg(path)
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
        if self.global_search_key(event, window, cx) {
            return;
        }
        if self.dialog.is_none()
            && event.keystroke.key == "w"
            && event.keystroke.modifiers.control
            && !event.keystroke.modifiers.alt
            && !event.keystroke.modifiers.shift
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
                crate::state::preview::Preview::Text(_) | crate::state::preview::Preview::Code(_)
            )
            && !modifiers.modified()
            && matches!(key, "up" | "down" | "home" | "end" | "pageup" | "pagedown")
        {
            let line_count = match &self.preview {
                crate::state::preview::Preview::Text(text) => text.split('\n').count(),
                crate::state::preview::Preview::Code(code) => code.text.split('\n').count(),
                _ => 1,
            };
            let page = (f32::from(self.preview_scroll.bounds().size.height) / 18.).floor() as usize;
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
        match key {
            "left" if modifiers.alt => self.back(cx),
            "right" if modifiers.alt => self.forward(cx),
            "up" if modifiers.alt => self.up(cx),
            "f5" if !modifiers.modified() => self.refresh(cx),
            "left" | "backspace" if !modifiers.modified() => self.up(cx),
            "right" if !modifiers.modified() => {
                if let Some(entry) = self
                    .selection
                    .primary()
                    .and_then(|index| self.entries.get(index))
                    .filter(|entry| entry.browsable())
                    .cloned()
                {
                    self.open(entry, cx);
                }
            }
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
                if !modifiers.control
                    && !modifiers.alt
                    && !modifiers.platform
                    && !self.entries.is_empty() =>
            {
                let height = f32::from(self.scroll.0.borrow().base_handle.bounds().size.height);
                let page = (height / self.row_height()).floor() as usize;
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

    /// Open a file living on a connected SSH host: download it into a local
    /// cache directory, then hand the local copy to xdg-open. The cache is
    /// wiped on application exit.
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
            let result = Command::new("xdg-open")
                .arg(&downloaded)
                .output()
                .map_err(|error| {
                    format!("{} {}: {error}", language.text("Cannot open"), entry.name)
                })
                .and_then(|output| {
                    if output.status.success() {
                        Ok(())
                    } else {
                        Err(format!(
                            "{} {}: {}",
                            language.text("Cannot open"),
                            entry.name,
                            String::from_utf8_lossy(&output.stderr).trim()
                        ))
                    }
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