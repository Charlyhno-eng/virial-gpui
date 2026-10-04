use crate::ui::components::reveal;
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
        if let Some(dialog) = &self.dialog {
            let heading = match dialog {
                Dialog::Name {
                    action: NameAction::Workspace { .. },
                    ..
                } => "Workspace name",
                Dialog::Name {
                    action: NameAction::Rename(_),
                    ..
                } => "Rename…",
                Dialog::Name {
                    action: NameAction::New { folder: true, .. },
                    ..
                } => "New folder…",
                Dialog::Name { .. } => "New file…",
                Dialog::Applications { .. } => "Open with…",
                Dialog::Trash(_) => "Move to Trash…",
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
            let confirm = matches!(dialog, Dialog::Name { .. } | Dialog::Trash(_));
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
        let mut actions = Vec::new();
        if let Some(entry) = &entry {
            actions.push(Action::Open);
            if self.selection.indices.len() == 1 {
                if !entry.directory {
                    actions.push(Action::OpenWith);
                }
                actions.push(Action::Rename);
                if !archive_entry {
                    actions.push(Action::Compress);
                }
                actions.push(Action::Properties);
            }
            actions.extend([Action::Cut, Action::Copy]);
            if !archive_entry {
                actions.push(Action::Trash);
            }
            actions.push(Action::CopyPath);
        } else if self.location.directory().is_some() {
            actions.extend([Action::NewFolder, Action::NewFile]);
        }
        if self.clipboard.is_some()
            && (entry.as_ref().is_some_and(|entry| entry.browsable())
                || self.location.directory().is_some())
        {
            actions.push(Action::Paste);
        }
        if entry
            .as_ref()
            .is_some_and(|entry| entry.directory && !archive_entry)
            || (entry.is_none()
                && self
                    .location
                    .directory()
                    .is_some_and(|path| crate::infrastructure::archive::split(path).is_none()))
        {
            actions.push(Action::AddWorkspace);
        }
        if self.location == crate::domain::location::Location::Workspaces {
            actions.push(Action::NewWorkspace);
        }
        actions.push(Action::Refresh);
        let bounds = window.viewport_size();
        let height = actions.len() as f32 * 28. + 9.;
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
                    .child(self.language.text(action.label()))
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.action(action, entry.clone(), window, cx)
                    }))
            }));
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
                .child(reveal(panel, "menu-reveal")),
        )
    }
}
