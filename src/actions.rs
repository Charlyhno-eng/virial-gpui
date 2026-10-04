//! Context actions and dialogs; filesystem work runs outside the UI thread.
use crate::{
    app::FileManager, applications::Application, files::Entry, operations::Operation,
    ui::input::NameInput,
};
use gpui::{AppContext, ClipboardItem, Context, Entity, KeyDownEvent, Pixels, Point, Window};
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::PathBuf,
};

#[derive(Clone)]
pub struct Menu {
    pub position: Point<Pixels>,
    pub entry: Option<Entry>,
}
#[derive(Clone)]
pub enum NameAction {
    Rename(PathBuf),
    New { directory: PathBuf, folder: bool },
}
pub enum Dialog {
    Name {
        action: NameAction,
        input: Entity<NameInput>,
    },
    Applications {
        entry: Entry,
        applications: Vec<Application>,
        loading: bool,
    },
    Trash(Entry),
    Properties {
        entry: Entry,
        details: String,
    },
}
#[derive(Clone, Copy)]
pub enum Action {
    Open,
    OpenWith,
    Copy,
    Cut,
    Paste,
    Rename,
    Trash,
    Compress,
    NewFolder,
    NewFile,
    CopyPath,
    Properties,
    Refresh,
}
impl Action {
    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::OpenWith => "Open with…",
            Self::Copy => "Copy",
            Self::Cut => "Cut",
            Self::Paste => "Paste",
            Self::Rename => "Rename…",
            Self::Trash => "Move to Trash…",
            Self::Compress => "Compress (.tar.gz)",
            Self::NewFolder => "New folder…",
            Self::NewFile => "New file…",
            Self::CopyPath => "Copy path",
            Self::Properties => "Properties",
            Self::Refresh => "Refresh",
        }
    }
}
impl FileManager {
    pub(crate) fn show_menu(
        &mut self,
        entry: Option<Entry>,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || self.dialog.is_some() {
            return;
        }
        self.selected = entry
            .as_ref()
            .and_then(|entry| self.entries.iter().position(|item| item.path == entry.path));
        self.focus.focus(window);
        self.menu = Some(Menu { position, entry });
        cx.notify();
    }
    pub(crate) fn action(
        &mut self,
        action: Action,
        entry: Option<Entry>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu = None;
        if self.busy {
            cx.notify();
            return;
        }
        let directory = entry
            .as_ref()
            .filter(|entry| entry.directory)
            .map(|entry| entry.path.clone())
            .or_else(|| self.location.directory().map(|path| path.to_path_buf()));
        match action {
            Action::Refresh => self.refresh(cx),
            Action::Paste => {
                if let (Some((source, cut)), Some(directory)) = (self.clipboard.clone(), directory)
                {
                    self.run_operation(
                        Operation::Paste {
                            source,
                            directory,
                            cut,
                        },
                        cx,
                    );
                }
            }
            Action::NewFolder | Action::NewFile => {
                if let Some(directory) = directory {
                    let folder = matches!(action, Action::NewFolder);
                    let text = self
                        .language
                        .text(if folder { "New folder" } else { "New file" })
                        .to_string();
                    let input = cx.new(|cx| NameInput::new(text, window, cx));
                    self.dialog = Some(Dialog::Name {
                        action: NameAction::New { directory, folder },
                        input,
                    });
                }
            }
            _ => {
                if let Some(entry) = entry {
                    match action {
                        Action::Open => self.open(entry, cx),
                        Action::Copy | Action::Cut => {
                            self.clipboard = Some((entry.path, matches!(action, Action::Cut)));
                        }
                        Action::CopyPath => cx.write_to_clipboard(ClipboardItem::new_string(
                            entry.path.display().to_string(),
                        )),
                        Action::Rename => {
                            // Keep the entire filename editable, including the extension.
                            if let Some(name) =
                                entry.path.file_name().and_then(|name| name.to_str())
                            {
                                let input =
                                    cx.new(|cx| NameInput::new(name.to_string(), window, cx));
                                self.dialog = Some(Dialog::Name {
                                    action: NameAction::Rename(entry.path),
                                    input,
                                });
                            } else {
                                self.error =
                                    Some(self.language.text("This name is not valid UTF-8").into());
                            }
                        }
                        Action::Trash => self.dialog = Some(Dialog::Trash(entry)),
                        Action::Compress => self.run_operation(Operation::Compress(entry.path), cx),
                        Action::OpenWith => {
                            self.dialog = Some(Dialog::Applications {
                                entry: entry.clone(),
                                applications: Vec::new(),
                                loading: true,
                            });
                            let home = self.home.clone();
                            let language = self.language;
                            let task = cx.background_executor().spawn(async move {
                                crate::applications::installed(&home, language)
                            });
                            cx.spawn(async move |view, cx| {
                                let applications = task.await;
                                let _ = view.update(cx, |view, cx| {
                                    if let Some(Dialog::Applications {
                                        entry: target,
                                        applications: list,
                                        loading,
                                    }) = &mut view.dialog
                                    {
                                        if target.path == entry.path {
                                            *list = applications;
                                            *loading = false;
                                            cx.notify();
                                        }
                                    }
                                });
                            })
                            .detach();
                        }
                        Action::Properties => {
                            self.dialog = Some(Dialog::Properties {
                                entry: entry.clone(),
                                details: self.language.text("Loading…").into(),
                            });
                            let path = entry.path.clone();
                            let language = self.language;
                            let task = cx.background_executor().spawn(async move {
                                fs::symlink_metadata(&path).map(|metadata| {
                                    let mut details = format!(
                                        "{}: {}\n{}: {}\n{}: {:o}\nUID: {} · GID: {}",
                                        language.text("Path"),
                                        path.display(),
                                        language.text("Size (bytes)"),
                                        metadata.len(),
                                        language.text("Permissions"),
                                        metadata.permissions().mode() & 0o7777,
                                        metadata.uid(),
                                        metadata.gid()
                                    );
                                    if let Ok(modified) = metadata.modified() {
                                        let date: chrono::DateTime<chrono::Utc> = modified.into();
                                        details.push_str(&format!(
                                            "\n{}: {} UTC",
                                            language.text("Modified"),
                                            date.format("%Y-%m-%d %H:%M:%S")
                                        ));
                                    }
                                    if metadata.is_symlink() {
                                        if let Ok(target) = fs::read_link(&path) {
                                            details.push_str(&format!(
                                                "\n{}: {}",
                                                language.text("Link target"),
                                                target.display()
                                            ));
                                        }
                                    }
                                    details
                                })
                            });
                            cx.spawn(async move |view, cx| {
                                let result = task.await;
                                let _ = view.update(cx, |view, cx| {
                                    if let Some(Dialog::Properties {
                                        entry: target,
                                        details,
                                    }) = &mut view.dialog
                                    {
                                        if target.path == entry.path {
                                            *details =
                                                result.unwrap_or_else(|error| error.to_string());
                                            cx.notify();
                                        }
                                    }
                                });
                            })
                            .detach();
                        }
                        _ => {}
                    }
                }
            }
        }
        cx.notify();
    }
    pub(crate) fn close_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dialog = None;
        self.focus.focus(window);
        cx.notify();
    }
    pub(crate) fn confirm_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let operation = match &self.dialog {
            Some(Dialog::Name { action, input }) => {
                let name = input.read(cx).text.clone();
                let directory = match action {
                    NameAction::Rename(path) => path.parent(),
                    NameAction::New { directory, .. } => Some(directory.as_path()),
                };
                if directory.is_none_or(|directory| {
                    crate::operations::named_path(directory, &name).is_err()
                }) {
                    self.error = Some(self.language.text("Invalid file name").into());
                    cx.notify();
                    return;
                }
                match action.clone() {
                    NameAction::Rename(source) => Some(Operation::Rename { source, name }),
                    NameAction::New { directory, folder } => Some(Operation::New {
                        directory,
                        name,
                        folder,
                    }),
                }
            }
            Some(Dialog::Trash(entry)) => Some(Operation::Trash(entry.path.clone())),
            _ => None,
        };
        if let Some(operation) = operation {
            self.close_dialog(window, cx);
            self.run_operation(operation, cx);
        }
    }
    pub(crate) fn run_operation(&mut self, operation: Operation, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        let origin = self.location.clone();
        let cut_source = match &operation {
            Operation::Paste {
                source, cut: true, ..
            } => Some(source.clone()),
            _ => None,
        };
        let recent_path = match &operation {
            Operation::Launch { file, .. } => Some(file.clone()),
            _ => None,
        };
        let data = self.data_home.clone();
        let task = cx.background_executor().spawn(async move {
            crate::operations::execute(operation)?;
            if let Some(path) = recent_path {
                crate::recent::record(&data, &path)?;
            }
            Ok::<_, std::io::Error>(())
        });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |view, cx| {
                view.busy = false;
                match result {
                    Ok(()) => {
                        if cut_source.as_ref().is_some_and(|source| {
                            view.clipboard
                                .as_ref()
                                .is_some_and(|(path, cut)| *cut && path == source)
                        }) {
                            view.clipboard = None;
                        }
                        if view.location == origin {
                            view.refresh(cx);
                        }
                    }
                    Err(error) => {
                        view.error = Some(format!(
                            "{}: {error}",
                            view.language.text("Operation failed")
                        ))
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub(crate) fn action_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        if self.dialog.is_some() {
            match key {
                "escape" => self.close_dialog(window, cx),
                "enter" if !modifiers.modified() => self.confirm_dialog(window, cx),
                _ => {}
            }
            return true;
        }
        if key == "escape" && self.menu.take().is_some() {
            cx.notify();
            return true;
        }
        let action = match key {
            "c" if modifiers.control => Some(Action::Copy),
            "x" if modifiers.control => Some(Action::Cut),
            "v" if modifiers.control => Some(Action::Paste),
            "n" if modifiers.control && modifiers.shift => Some(Action::NewFolder),
            "f2" => Some(Action::Rename),
            "delete" if !modifiers.modified() => Some(Action::Trash),
            _ => None,
        };
        if let Some(action) = action {
            let entry = if matches!(action, Action::Paste | Action::NewFolder) {
                None
            } else {
                self.selected
                    .and_then(|index| self.entries.get(index))
                    .cloned()
            };
            self.action(action, entry, window, cx);
            return true;
        }
        false
    }
}
