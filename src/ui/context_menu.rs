use crate::{
    actions::{Action, Dialog, NameAction},
    app::FileManager,
    theme::*,
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
    pub(super) fn context_overlay(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        if let Some(dialog) = &self.dialog {
            let heading = match dialog {
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
                .w(px(540.))
                .max_w_full()
                .max_h_full()
                .p_5()
                .flex()
                .flex_col()
                .gap_4()
                .rounded_lg()
                .bg(color(SURFACE))
                .border_1()
                .border_color(color(BORDER))
                .child(div().text_size(px(18.)).child(self.language.text(heading)));
            if let Some(error) = &self.error {
                content = content.child(div().text_color(color(ERROR)).child(error.clone()));
            }
            content = match dialog {
                Dialog::Name { input, .. } => content
                    .child(
                        div().text_size(px(12.)).text_color(color(MUTED)).child(
                            self.language
                                .text("Edit the full name, including the extension"),
                        ),
                    )
                    .child(input.clone()),
                Dialog::Trash(entry) => content.child(entry.name.clone()).child(
                    self.language
                        .text("This item will be moved to the desktop Trash"),
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
                                        crate::operations::Operation::Launch {
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
                    .child(content),
            );
        }
        let menu = self.menu.as_ref()?;
        let entry = menu.entry.clone();
        let mut actions = Vec::new();
        if let Some(entry) = &entry {
            actions.push(Action::Open);
            if !entry.directory {
                actions.push(Action::OpenWith);
            }
            actions.extend([
                Action::Cut,
                Action::Copy,
                Action::Rename,
                Action::Trash,
                Action::Compress,
                Action::CopyPath,
                Action::Properties,
            ]);
        } else if self.location.directory().is_some() {
            actions.extend([Action::NewFolder, Action::NewFile]);
        }
        if self.clipboard.is_some()
            && (entry.as_ref().is_some_and(|entry| entry.directory)
                || self.location.directory().is_some())
        {
            actions.push(Action::Paste);
        }
        actions.push(Action::Refresh);
        let bounds = window.viewport_size();
        let height = actions.len() as f32 * 34. + 12.;
        let x = menu.position.x.min(bounds.width - px(250.)).max(px(0.));
        let y = menu.position.y.min(bounds.height - px(height)).max(px(0.));
        let panel = div()
            .id("context-menu")
            .occlude()
            .absolute()
            .left(x)
            .top(y)
            .w(px(250.))
            .p_1()
            .rounded_md()
            .bg(color(SURFACE))
            .border_1()
            .border_color(color(BORDER))
            .children(actions.into_iter().enumerate().map(|(index, action)| {
                let entry = entry.clone();
                div()
                    .id(("context-action", index))
                    .h(px(34.))
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
                .child(panel),
        )
    }
}
