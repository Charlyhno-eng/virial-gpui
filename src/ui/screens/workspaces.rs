use crate::{
    app::FileManager,
    infrastructure::workspaces::Edit,
    ui::{icons::icon, theme::*},
};
use gpui::{Context, Div, div, prelude::*, px};

fn button(id: impl Into<gpui::ElementId>, label: &'static str) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .px_3()
        .py_2()
        .rounded_md()
        .bg(color(HOVER))
        .cursor_pointer()
        .hover(|style| style.bg(color(SELECTED)))
        .child(label)
}

impl FileManager {
    pub(crate) fn workspace_view(&self, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let mut view = div()
            .id("workspace-view")
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_y_scroll()
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(18.))
                            .child(self.language.text("Workspaces")),
                    )
                    .child(
                        button("new-workspace", self.language.text("New workspace…")).on_click(
                            cx.listener(|view, _, window, cx| {
                                view.workspace_dialog(None, window, cx)
                            }),
                        ),
                    ),
            )
            .child(
                div().text_color(color(MUTED)).child(
                    self.language
                        .text("Logical groups of folders · Ctrl+W / Escape returns"),
                ),
            );
        if let Some(error) = &self.error {
            view = view.child(div().text_color(color(ERROR)).child(error.clone()));
        }
        if self.busy {
            view = view.child(self.language.text("Working…"));
        }
        if self.loading {
            return view.child(self.language.text("Loading…"));
        }
        if self.workspaces.is_empty() {
            return view.child(self.language.text("No workspaces yet")).child(
                self.language
                    .text("Browse a folder and use Add folder to workspace in the toolbar"),
            );
        }
        for (index, summary) in self.workspaces.iter().enumerate() {
            let name = summary.workspace.name.clone();
            let mut card = div()
                .flex_shrink_0()
                .p_4()
                .flex()
                .flex_col()
                .gap_3()
                .rounded_lg()
                .bg(translucent(SURFACE, 0.6))
                .border_1()
                .border_color(color(BORDER))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_ellipsis()
                                .text_size(px(16.))
                                .child(name.clone()),
                        )
                        .child(
                            button(
                                ("remove-workspace", index),
                                self.language.text("Remove workspace"),
                            )
                            .on_click(cx.listener(
                                move |view, _, _, cx| {
                                    view.edit_workspace(Edit::Remove(name.clone()), cx)
                                },
                            )),
                        ),
                )
                .child(
                    div()
                        .max_w_full()
                        .min_w_0()
                        .text_color(color(MUTED))
                        .child(self.language.counts(summary.directories, summary.files)),
                )
                .child(
                    div().text_color(color(MUTED)).child(
                        self.language
                            .text("Counts and activity cover immediate folder contents"),
                    ),
                );
            let mut folders = div().flex().flex_wrap().gap_2();
            for (folder_index, folder) in summary.workspace.folders.iter().enumerate() {
                let path = folder.clone();
                let remove_path = path.clone();
                let name = summary.workspace.name.clone();
                folders = folders.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .p_2()
                        .rounded_md()
                        .border_1()
                        .border_color(color(BORDER))
                        .child(
                            div()
                                .id((
                                    gpui::SharedString::from(format!("workspace-folder-{index}")),
                                    folder_index,
                                ))
                                .flex()
                                .items_center()
                                .gap_2()
                                .cursor_pointer()
                                .p_2()
                                .hover(|style| style.bg(color(HOVER)))
                                .child(icon("folder", 18., ACCENT_BLUE))
                                .child(
                                    div()
                                        .min_w_0()
                                        .text_ellipsis()
                                        .child(path.display().to_string()),
                                )
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    view.focus.focus(window);
                                    view.navigate(path.clone(), cx);
                                })),
                        )
                        .child(
                            button(
                                (
                                    gpui::SharedString::from(format!("remove-folder-{index}")),
                                    folder_index,
                                ),
                                self.language.text("Remove association"),
                            )
                            .on_click(cx.listener(
                                move |view, _, _, cx| {
                                    view.edit_workspace(
                                        Edit::RemoveFolder {
                                            name: name.clone(),
                                            folder: remove_path.clone(),
                                        },
                                        cx,
                                    )
                                },
                            )),
                        ),
                );
            }
            card = card.child(folders);
            if summary.workspace.folders.is_empty() {
                card = card.child(
                    self.language
                        .text("Browse a folder and use Add folder to workspace in the toolbar"),
                );
            }
            for folder in &summary.unavailable {
                card = card.child(div().text_color(color(ERROR)).child(format!(
                    "{}: {}",
                    self.language.text("Folder unavailable"),
                    folder.display()
                )));
            }
            card = card.child(self.language.text("Recently modified files"));
            if summary.recent.is_empty() {
                card = card.child(
                    div()
                        .text_color(color(MUTED))
                        .child(self.language.text("No recent files")),
                );
            }
            for (recent_index, (modified, entry)) in summary.recent.iter().enumerate() {
                let entry = entry.clone();
                let date = chrono::DateTime::<chrono::Utc>::from(*modified)
                    .format("%Y-%m-%d %H:%M UTC")
                    .to_string();
                card = card.child(
                    div()
                        .id((
                            gpui::SharedString::from(format!("workspace-recent-{index}")),
                            recent_index,
                        ))
                        .px_2()
                        .py_1()
                        .text_ellipsis()
                        .rounded_md()
                        .cursor_pointer()
                        .hover(|style| style.bg(color(HOVER)))
                        .child(format!("{} · {date}", entry.path.display()))
                        .on_click(cx.listener(move |view, _, _, cx| view.open(entry.clone(), cx))),
                );
            }
            view = view.child(card);
        }
        view
    }
}
