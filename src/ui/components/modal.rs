use crate::infrastructure::image_edit::{ExportFormat, ImageEdit};
use crate::ui::components::reveal;
use crate::ui::icons::icon;
use crate::{
    app::FileManager,
    state::actions::{Action, Dialog, NameAction},
    ui::theme::*,
};
use gpui::{Context, Div, MouseButton, Window, div, prelude::*, px};

fn button(id: &'static str, label: &'static str) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .px_4()
        .py_2()
        .rounded_md()
        .bg(color(HOVER))
        .cursor_pointer()
        .hover(|style| style.bg(color(SELECTED)))
        .child(label)
}
impl FileManager {
    pub(crate) fn context_overlay(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        if self.ssh.menu_open {
            return self.remote_menu(cx);
        }
        if self.ssh.dialog_input.is_some() {
            return Some(self.ssh_dialog(cx));
        }
        if let Some(dialog) = &self.dialog {
            let heading = match dialog {
                Dialog::ImageExport { edit, .. } => match edit {
                    ImageEdit::Convert(_) => "Convert image…",
                    ImageEdit::RemoveBackground => "Remove background…",
                },
                Dialog::Name {
                    action: NameAction::Workspace { .. },
                    ..
                } => "Workspace name",
                Dialog::Name {
                    action: NameAction::New { folder: true, .. },
                    ..
                } => "New folder…",
                Dialog::Name { .. } => "New file…",
                Dialog::Applications { .. } => "Open with…",
                Dialog::Trash(_) => "Move to Trash…",
                Dialog::EmptyTrash => "Empty Trash…",
                Dialog::Undo { .. } => "Restore deleted items?",
                Dialog::Properties { .. } => "Properties",
            };
            let mut content = div()
                .id("dialog-panel")
                .occlude()
                .w(px(460.))
                .max_w_full()
                .max_h_full()
                .p_4()
                .flex()
                .flex_col()
                .gap_3()
                .rounded_lg()
                .bg(color(SURFACE))
                .border_1()
                .border_color(color(BORDER))
                .shadow_lg()
                .child(div().text_size(px(16.)).child(self.language.text(heading)));
            if let Some(error) = &self.error {
                content = content.child(div().text_color(color(ERROR)).child(error.clone()));
            }
            content = match dialog {
                Dialog::ImageExport { source, edit, input } => {
                    let mut panel = content
                        .child(div().text_ellipsis().child(source.display().to_string()))
                        .child(self.language.text("Save a new image beside the original; existing files are never overwritten"));
                    if let ImageEdit::Convert(selected) = edit {
                        panel = panel.child(div().flex().gap_2().children(
                            ExportFormat::ALL.into_iter().enumerate().map(|(index, format)| {
                                div().id(("image-export-format", index)).px_3().py_2().rounded_md()
                                    .bg(color(if *selected == format { SELECTED } else { HOVER }))
                                    .cursor_pointer().child(format.label())
                                    .on_click(cx.listener(move |view, _, window, cx| view.image_export_format(format, window, cx)))
                            })
                        )).child(self.language.text("Animated images export their first frame. JPEG uses a white background for transparency."));
                    } else {
                        panel = panel.child(self.language.text("Creates a transparent PNG using local rembg. The first use downloads a model; setup is described in README.md."));
                    }
                    panel.child(self.language.text("Output file name")).child(input.clone())
                },
                Dialog::Name { action: NameAction::Workspace { folder, names }, input } => content
                    .child(self.language.text(if folder.is_some() { "Use an existing name to add this folder, or a new name to create a workspace" } else { "Create an empty workspace, then add folders from the context menu" }))
                    .children(folder.as_ref().map(|path| div().text_color(color(MUTED)).child(path.display().to_string())))
                    .child(input.clone())
                    .child(div().id("existing-workspaces").max_h(px(180.)).overflow_y_scroll()
                        .children(names.iter().enumerate().map(|(index, name)| {
                            let name = name.clone();
                            let folder = folder.clone();
                            div().id(("existing-workspace", index)).px_3().py_2().rounded_md()
                                .cursor_pointer().hover(|style| style.bg(color(HOVER)))
                                .child(name.clone()).on_click(cx.listener(move |view, _, window, cx| {
                                    view.close_dialog(window, cx);
                                    view.edit_workspace(crate::infrastructure::workspaces::Edit::Add { name: name.clone(), folder: folder.clone() }, cx);
                                }))
                        }))),
                Dialog::Name { input, .. } => content
                    .child(
                        div().text_size(px(11.)).text_color(color(MUTED)).child(
                            self.language
                                .text("Edit the full name, including the extension"),
                        ),
                    )
                    .child(input.clone()),
                Dialog::Trash(entries) => content
                    .child(self.language.selected_count(entries.len()))
                    .child(
                        div()
                            .id("trash-selection")
                            .max_h(px(200.))
                            .overflow_y_scroll()
                            .children(
                                entries
                                    .iter()
                                    .map(|entry| div().text_ellipsis().child(entry.name.clone())),
                            ),
                    )
                    .child(
                        self.language
                            .text("Selected items will be moved to the desktop Trash"),
                    ),
                Dialog::EmptyTrash => content
                    .child(self.language.text("Permanently delete all items in the Trash? This cannot be undone.")),
                Dialog::Undo { summary, .. } => {
                    let count = summary.split('\t').nth(1).unwrap_or("?");
                    content.child(format!("{} {count}", self.language.text("Deleted items:")))
                        .child(self.language.text("Restore the entire batch? Existing files and later edits are protected."))
                },
                Dialog::Properties { entry, details } => content.child(entry.name.clone()).child(
                    div()
                        .id("property-details")
                        .max_h(px(350.))
                        .overflow_y_scroll()
                        .child(details.clone()),
                ),
                Dialog::Applications {
                    entry,
                    applications,
                    loading,
                } => {
                    let mut list = div()
                        .id("applications-list")
                        .max_h(px(350.))
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .gap_1();
                    if *loading {
                        list = list.child(self.language.text("Loading…"));
                    } else if applications.is_empty() {
                        list = list.child(self.language.text("No applications found"));
                    }
                    for (index, application) in applications.iter().enumerate() {
                        let desktop = application.desktop.clone();
                        let file = entry.path.clone();
                        list = list.child(
                            div()
                                .id(("application", index))
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .cursor_pointer()
                                .hover(|style| style.bg(color(HOVER)))
                                .child(application.name.clone())
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    view.close_dialog(window, cx);
                                    view.run_operation(
                                        crate::infrastructure::operations::Operation::Launch {
                                            desktop: desktop.clone(),
                                            file: file.clone(),
                                        },
                                        cx,
                                    );
                                })),
                        );
                    }
                    content
                        .child(div().text_ellipsis().child(entry.name.clone()))
                        .child(list)
                }
            };
            let confirm = matches!(
                dialog,
                Dialog::Name { .. }
                    | Dialog::Trash(_)
                    | Dialog::EmptyTrash
                    | Dialog::ImageExport { .. }
                    | Dialog::Undo { .. }
            );
            content = content.child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        button(
                            "cancel-dialog",
                            self.language.text(if confirm { "Cancel" } else { "Close" }),
                        )
                        .on_click(cx.listener(|view, _, window, cx| view.close_dialog(window, cx))),
                    )
                    .when(confirm, |bar| {
                        bar.child(
                            button("confirm-dialog", self.language.text("Confirm")).on_click(
                                cx.listener(|view, _, window, cx| view.confirm_dialog(window, cx)),
                            ),
                        )
                    }),
            );
            return Some(
                div()
                    .occlude()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(gpui::rgba(0x00000080))
                    .child(reveal(content, "dialog-reveal")),
            );
        }
        let menu = self.menu.as_ref()?;
        let entry = menu.entry.clone();
        let archive_entry = entry
            .as_ref()
            .is_some_and(|entry| crate::infrastructure::archive::is_member(&entry.path));
        let mut groups: Vec<Vec<Action>> = Vec::new();
        let trash = self.location == crate::domain::location::Location::Trash;
        let remote = self.location.remote().is_some();
        let single = self.selection.indices.len() == 1;
        let paste = !trash
            && !remote
            && self.clipboard.is_some()
            && (entry.as_ref().is_some_and(|entry| entry.browsable())
                || self.location.directory().is_some());
        let workspace = !trash
            && !remote
            && (entry
                .as_ref()
                .is_some_and(|entry| entry.directory && !archive_entry)
                || (entry.is_none()
                    && self.location.directory().is_some_and(|path| {
                        crate::infrastructure::archive::split(path).is_none()
                    })));
        if remote {
            if entry.is_some() {
                groups.push(vec![Action::Open]);
                if single {
                    groups.push(vec![Action::Rename]);
                    groups.push(vec![Action::Trash]);
                }
            } else {
                groups.push(vec![Action::NewFolder]);
            }
        } else if trash {
            if entry.is_some() {
                groups.push(vec![Action::Restore]);
            }
            groups.push(vec![Action::EmptyTrash]);
        } else if let Some(item) = &entry {
            let mut open = vec![Action::Open];
            if single && !item.directory {
                open.push(Action::OpenWith);
            }
            groups.push(open);
            let mut edit = vec![Action::Copy, Action::Cut];
            if paste {
                edit.push(Action::Paste);
            }
            if single {
                if !archive_entry {
                    edit.push(Action::Duplicate);
                }
                edit.push(Action::Rename);
            }
            groups.push(edit);
            if workspace {
                groups.push(vec![Action::AddWorkspace]);
            }
            if single && !archive_entry {
                groups.push(vec![Action::Compress]);
            }
            if !archive_entry {
                groups.push(vec![Action::Trash]);
            }
            groups.push(vec![Action::CopyPath]);
            if single {
                groups.push(vec![Action::Properties]);
            }
        } else {
            if self.location.directory().is_some() {
                groups.push(vec![Action::NewFolder, Action::NewFile]);
            }
            if paste {
                groups.push(vec![Action::Paste]);
            }
            if workspace {
                groups.push(vec![Action::AddWorkspace]);
            }
            if self.location == crate::domain::location::Location::Workspaces {
                groups.push(vec![Action::NewWorkspace]);
            }
        }
        let mut actions = Vec::new();
        for group in groups {
            if !actions.is_empty() {
                actions.push(None);
            }
            actions.extend(group.into_iter().map(Some));
        }
        let bounds = window.viewport_size();
        let height = actions
            .iter()
            .map(|action| if action.is_some() { 28. } else { 9. })
            .sum::<f32>()
            + 10.;
        let compression_top = actions
            .iter()
            .take_while(|action| **action != Some(Action::Compress))
            .map(|action| if action.is_some() { 28. } else { 9. })
            .sum::<f32>();
        let x = menu.position.x.min(bounds.width - px(220.)).max(px(0.));
        let y = menu.position.y.min(bounds.height - px(height)).max(px(0.));
        let panel = div()
            .id("context-menu")
            .occlude()
            .absolute()
            .left(x)
            .top(y)
            .w(px(220.))
            .p_1()
            .rounded_md()
            .bg(color(SURFACE))
            .border_1()
            .border_color(color(BORDER))
            .shadow_md()
            .children(actions.into_iter().enumerate().map(|(index, action)| {
                let Some(action) = action else {
                    return div()
                        .h(px(9.))
                        .px_2()
                        .flex()
                        .items_center()
                        .child(div().w_full().h(px(1.)).bg(color(BORDER)))
                        .into_any_element();
                };
                let entry = entry.clone();
                div()
                    .id(("context-action", index))
                    .h(px(28.))
                    .px_3()
                    .flex()
                    .items_center()
                    .rounded_sm()
                    .cursor_pointer()
                    .hover(|style| style.bg(color(HOVER)))
                    .on_hover(cx.listener(move |view, hovered, _, cx| {
                        if *hovered && let Some(menu) = view.menu.as_mut() {
                            let open = action == Action::Compress;
                            if menu.compression_open != open {
                                menu.compression_open = open;
                                cx.notify();
                            }
                        }
                    }))
                    .child(icon(action.icon(), 14., MUTED))
                    .child(
                        div()
                            .ml_2()
                            .flex_1()
                            .min_w_0()
                            .text_ellipsis()
                            .child(self.language.text(action.label())),
                    )
                    .when(action == Action::Compress, |row| {
                        row.child(icon("forward", 12., MUTED))
                    })
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if action == Action::Compress {
                            if let Some(menu) = view.menu.as_mut() {
                                menu.compression_open = true;
                            }
                            cx.notify();
                        } else {
                            view.action(action, entry.clone(), window, cx);
                        }
                    }))
                    .into_any_element()
            }));
        let submenu = menu.compression_open.then(|| {
            let width = px(190.);
            let submenu_x = if x + px(220.) + width <= bounds.width {
                x + px(220.)
            } else {
                (x - width).max(px(0.))
            };
            let submenu_y =
                (y + px(compression_top + 4.)).min((bounds.height - px(122.)).max(px(0.)));
            div()
                .id("compression-submenu")
                .occlude()
                .absolute()
                .left(submenu_x)
                .top(submenu_y)
                .w(width)
                .p_1()
                .rounded_md()
                .bg(color(SURFACE))
                .border_1()
                .border_color(color(BORDER))
                .shadow_md()
                .children(
                    crate::infrastructure::compression::ArchiveFormat::ALL
                        .into_iter()
                        .enumerate()
                        .map(|(index, format)| {
                            let entry = entry.clone();
                            div()
                                .id(("compression-format", index))
                                .h(px(28.))
                                .px_3()
                                .flex()
                                .items_center()
                                .rounded_sm()
                                .cursor_pointer()
                                .hover(|style| style.bg(color(HOVER)))
                                .child(format.label())
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    view.action(
                                        Action::CompressAs(format),
                                        entry.clone(),
                                        window,
                                        cx,
                                    );
                                }))
                        }),
                )
        });
        Some(
            div()
                .absolute()
                .inset_0()
                .child(
                    div()
                        .id("menu-dismiss")
                        .occlude()
                        .absolute()
                        .inset_0()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|view, _, _, cx| {
                                view.menu = None;
                                cx.notify();
                            }),
                        )
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(|view, _, _, cx| {
                                view.menu = None;
                                cx.notify();
                            }),
                        ),
                )
                .child(reveal(panel, "menu-reveal"))
                .children(submenu),
        )
    }
}

impl Action {
    fn icon(self) -> &'static str {
        match self {
            Self::Restore => "restore",
            Self::EmptyTrash => "trash",
            Self::Open | Self::OpenWith => "open",
            Self::Copy | Self::CopyPath | Self::Duplicate => "copy",
            Self::Cut => "cut",
            Self::Paste => "paste",
            Self::Rename => "edit",
            Self::Trash => "trash",
            Self::Compress | Self::CompressAs(_) => "compress",
            Self::NewFolder | Self::AddWorkspace => "folder",
            Self::NewFile => "file",
            Self::Properties => "info",
            Self::NewWorkspace => "folder-plus",
        }
    }
}

// ---------------------------------------------------------------------------
// Remote (SSH) menu and connect dialog, opened from the status-bar indicator.
// ---------------------------------------------------------------------------

pub(crate) struct StatusTooltip(pub(crate) String);

impl gpui::Render for StatusTooltip {
    fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .rounded_sm()
            .bg(color(SURFACE))
            .border_1()
            .border_color(color(BORDER))
            .text_color(color(TEXT))
            .text_size(px(11.))
            .child(self.0.clone())
    }
}

enum RemoteMenuAction {
    Connect(crate::infrastructure::ssh::HostId),
    OpenDialog,
    Disconnect,
}

impl RemoteMenuAction {
    fn is_disconnect(&self) -> bool {
        matches!(self, Self::Disconnect)
    }
}

impl FileManager {
    /// The small menu anchored bottom-left under the remote indicator, in the
    /// spirit of the VS Code remote indicator menu.
    fn remote_menu(&self, cx: &mut Context<Self>) -> Option<Div> {
        let mut entries: Vec<(String, RemoteMenuAction)> = Vec::new();
        for host in &self.ssh.hosts {
            let label = if self.ssh.connected(&host.id) {
                format!("{} ·", host.display())
            } else {
                host.display()
            };
            entries.push((label, RemoteMenuAction::Connect(host.id.clone())));
        }
        entries.push((
            self.language.text("Connect to Host…").into(),
            RemoteMenuAction::OpenDialog,
        ));
        if self.location.remote().is_some() {
            entries.push((
                self.language.text("Close Remote Connection").into(),
                RemoteMenuAction::Disconnect,
            ));
        }
        let panel = div()
            .id("remote-menu")
            .occlude()
            .absolute()
            .left(px(6.))
            .bottom(px(
                crate::ui::components::sidebar::SIDEBAR_FOOTER_HEIGHT + 4.
            ))
            .w(px(260.))
            .p_1()
            .rounded_md()
            .bg(color(SURFACE))
            .border_1()
            .border_color(color(BORDER))
            .shadow_md()
            .children(
                entries
                    .into_iter()
                    .enumerate()
                    .map(|(index, (label, action))| {
                        div()
                            .id(("remote-menu-action", index))
                            .h(px(26.))
                            .px_3()
                            .flex()
                            .items_center()
                            .rounded_sm()
                            .cursor_pointer()
                            .text_size(px(10.5))
                            .text_color(color(if action.is_disconnect() { ERROR } else { TEXT }))
                            .hover(|style| style.bg(color(HOVER)))
                            .child(label)
                            .on_click(cx.listener(move |view, _, window, cx| match &action {
                                RemoteMenuAction::Connect(id) => {
                                    view.connect_saved_host(id.clone(), window, cx)
                                }
                                RemoteMenuAction::OpenDialog => view.open_ssh_dialog(window, cx),
                                RemoteMenuAction::Disconnect => view.disconnect_remote(cx),
                            }))
                    }),
            );
        Some(
            div()
                .absolute()
                .inset_0()
                .child(
                    div()
                        .id("remote-menu-dismiss")
                        .occlude()
                        .absolute()
                        .inset_0()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|view, _, _, cx| {
                                view.close_remote_menu(cx);
                            }),
                        ),
                )
                .child(reveal(panel, "remote-menu-reveal")),
        )
    }

    /// The SSH connect dialog: target, authentication route (agent / key file
    /// / password) and the per-route field. The secret stays in memory only.
    fn ssh_dialog(&self, cx: &mut Context<Self>) -> Div {
        use crate::state::ssh::SshAuthMode;
        let auth_chip =
            |cx: &mut Context<Self>, id: &'static str, mode: SshAuthMode, label: &'static str| {
                let active = self.ssh.dialog_auth == mode;
                button(id, self.language.text(label))
                    .when(active, |chip| chip.bg(color(SELECTED)))
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.select_ssh_auth(mode, window, cx)
                    }))
            };
        let mut content = div()
            .id("ssh-dialog-panel")
            .occlude()
            .w(px(420.))
            .max_w_full()
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
            .rounded_lg()
            .bg(color(SURFACE))
            .border_1()
            .border_color(color(BORDER))
            .shadow_lg()
            .child(
                div()
                    .text_size(px(16.))
                    .child(self.language.text("Connect to Host")),
            )
            .child(div().text_size(px(11.)).text_color(color(MUTED)).child(
                self.language.text(
                    "Format: user@host[:port] — keys and the SSH agent are tried automatically",
                ),
            ))
            .child(
                div()
                    .id("ssh-auth-modes")
                    .flex()
                    .gap_1()
                    .child(auth_chip(cx, "auth-agent", SshAuthMode::Agent, "Agent"))
                    .child(auth_chip(cx, "auth-key", SshAuthMode::Key, "Key file"))
                    .child(auth_chip(
                        cx,
                        "auth-password",
                        SshAuthMode::Password,
                        "Password",
                    )),
            );
        if let Some(input) = &self.ssh.dialog_input {
            content = content.child(input.clone());
        }
        if self.ssh.dialog_auth != SshAuthMode::Agent
            && let Some(aux) = &self.ssh.dialog_aux
        {
            content = content.child(aux.clone());
        }
        if let Some(error) = &self.ssh.error {
            content = content.child(div().text_color(color(ERROR)).child(error.clone()));
        }
        let connecting = matches!(
            self.ssh.activity,
            crate::state::ssh::SshActivity::Connecting(_)
        );
        content =
            content.child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(button("cancel-ssh", self.language.text("Cancel")).on_click(
                        cx.listener(|view, _, window, cx| view.close_ssh_dialog(window, cx)),
                    ))
                    .child(
                        button(
                            "confirm-ssh",
                            self.language.text(if connecting {
                                "Connecting…"
                            } else {
                                "Connect"
                            }),
                        )
                        .on_click(
                            cx.listener(|view, _, window, cx| view.confirm_ssh_dialog(window, cx)),
                        ),
                    ),
            );
        div()
            .occlude()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x00000080))
            .child(reveal(content, "ssh-dialog-reveal"))
    }
}
