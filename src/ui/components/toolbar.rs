use crate::ui::components::{navigation_button, reveal, toolbar_button};
use crate::{app::FileManager, ui::icons::icon, ui::theme::*};
use gpui::{Context, Div, MouseButton, Window, div, prelude::*, px};

impl FileManager {
    pub(crate) fn toolbar(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .px_4()
            .py_2()
            .flex_shrink_0()
            .bg(translucent(SURFACE, 0.35))
            .border_b_1()
            .border_color(color(BORDER))
            .child(
                navigation_button(
                    "back",
                    "back",
                    self.language.text("Back"),
                    self.history.back().is_some(),
                )
                .on_click(cx.listener(|view, _, window, cx| {
                    view.focus.focus(window);
                    view.back(cx);
                })),
            )
            .child(
                navigation_button(
                    "forward",
                    "forward",
                    self.language.text("Forward"),
                    self.history.forward().is_some(),
                )
                .on_click(cx.listener(|view, _, window, cx| {
                    view.focus.focus(window);
                    view.forward(cx);
                })),
            )
            .child(
                navigation_button(
                    "up",
                    "up",
                    self.language.text("Up"),
                    self.location
                        .directory()
                        .and_then(|path| path.parent())
                        .is_some(),
                )
                .on_click(cx.listener(|view, _, window, cx| {
                    view.focus.focus(window);
                    view.up(cx);
                })),
            )
            .child(
                div()
                    .id("breadcrumbs")
                    .flex()
                    .items_center()
                    .flex_1()
                    .min_w(px(140.))
                    .overflow_x_scroll()
                    .children(
                        self.location
                            .breadcrumbs(&self.home, self.language)
                            .into_iter()
                            .enumerate()
                            .map(|(index, (label, path))| {
                                let current = path == self.location;
                                div()
                                    .flex()
                                    .items_center()
                                    .flex_shrink_0()
                                    .when(index > 0, |crumb| {
                                        crumb.child(icon("forward", 13., MUTED))
                                    })
                                    .child(
                                        div()
                                            .id(("crumb", index))
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .px_2()
                                            .py_1()
                                            .rounded_sm()
                                            .border_b_1()
                                            .border_color(gpui::transparent_black())
                                            .cursor_pointer()
                                            .text_size(px(11.))
                                            .text_color(color(if current { TEXT } else { MUTED }))
                                            .hover(|style| {
                                                style
                                                    .bg(translucent(ACCENT_BLUE, 0.06))
                                                    .border_color(translucent(ACCENT_BLUE, 0.24))
                                                    .text_color(color(TEXT))
                                            })
                                            .when(index == 0, |crumb| {
                                                crumb.child(icon("home", 14., MUTED))
                                            })
                                            .child(label)
                                            .when_some(
                                                path.directory().map(|path| path.to_path_buf()),
                                                |crumb, directory| {
                                                    self.drop_target(crumb, directory, cx)
                                                },
                                            )
                                            .on_click(cx.listener(move |view, _, window, cx| {
                                                view.focus.focus(window);
                                                view.navigate_location(path.clone(), cx);
                                            })),
                                    )
                            }),
                    ),
            )
            .child(
                div()
                    .id("search-field")
                    .w(px(220.))
                    .max_w_full()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(icon("search", 14., MUTED))
                    .child(self.search_input.clone()),
            )
            .child(
                toolbar_button(
                    "refresh",
                    "refresh",
                    self.language.text("Refresh"),
                    true,
                    false,
                )
                .on_click(cx.listener(|view, _, _, cx| view.refresh(cx))),
            )
            .child(
                toolbar_button(
                    "display",
                    "view",
                    self.language.text("Display"),
                    true,
                    self.display_menu,
                )
                .child(icon("down", 12., MUTED))
                .on_click(cx.listener(|view, _, window, cx| {
                    view.display_menu = !view.display_menu;
                    view.menu = None;
                    view.focus.focus(window);
                    cx.notify();
                })),
            )
            .when(window.is_fullscreen(), |bar| {
                bar.child(
                    toolbar_button(
                        "exit-fullscreen",
                        "fullscreen",
                        self.language.text("Exit full screen · F11"),
                        true,
                        true,
                    )
                    .on_click(|_, window, cx| {
                        window.toggle_fullscreen();
                        window.refresh();
                        cx.stop_propagation();
                    }),
                )
            })
    }

    pub(crate) fn display_overlay(&self, window: &Window, cx: &mut Context<Self>) -> Option<Div> {
        if !self.display_menu {
            return None;
        }
        let mut panel = div()
            .id("display-options")
            .occlude()
            .absolute()
            .right_3()
            .top(px(if window.is_fullscreen() { 48. } else { 80. }))
            .w(px(248.))
            .max_w_full()
            .p_2()
            .rounded_md()
            .bg(color(SURFACE))
            .border_1()
            .border_color(color(BORDER))
            .shadow_md()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                toolbar_button(
                    "display-compact",
                    "view",
                    self.language.text("Compact list"),
                    true,
                    self.compact_view,
                )
                .on_click(cx.listener(|view, _, _, cx| {
                    view.compact_view = !view.compact_view;
                    cx.notify();
                })),
            )
            .child(
                toolbar_button(
                    "display-hidden",
                    "eye",
                    self.language.text("Hidden files"),
                    true,
                    self.hidden,
                )
                .on_click(cx.listener(|view, _, _, cx| view.toggle_hidden(cx))),
            )
            .child(
                toolbar_button(
                    "display-folder-details",
                    "info",
                    self.language.text("Folder information"),
                    self.location.directory().is_some(),
                    self.folder_details,
                )
                .on_click(cx.listener(|view, _, _, cx| {
                    if view.location.directory().is_some() {
                        view.folder_details = !view.folder_details;
                        cx.notify();
                    }
                })),
            )
            .child(
                toolbar_button(
                    "display-preview",
                    "image",
                    self.language.text("Preview"),
                    self.selection.primary().is_some(),
                    self.details_open && !self.folder_details,
                )
                .on_click(cx.listener(|view, _, _, cx| {
                    if view.selection.primary().is_some() {
                        view.sync_preview(cx);
                        if view.folder_details {
                            view.folder_details = false;
                            view.details_open = true;
                        } else {
                            view.details_open = !view.details_open;
                        }
                        view.preview_expanded = false;
                        cx.notify();
                    }
                })),
            )
            .child(div().h(px(1.)).my_1().bg(color(BORDER)));
        if let Some(folder) = self.location.directory().map(|path| path.to_path_buf()) {
            panel = panel.child(
                toolbar_button(
                    "add-workspace-folder",
                    "folder",
                    self.language.text("Add folder to workspace"),
                    !self.busy,
                    false,
                )
                .on_click(cx.listener(move |view, _, window, cx| {
                    view.display_menu = false;
                    view.workspace_dialog(Some(folder.clone()), window, cx);
                })),
            );
        }
        Some(
            div()
                .absolute()
                .inset_0()
                .child(
                    div()
                        .id("display-dismiss")
                        .occlude()
                        .absolute()
                        .inset_0()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|view, _, _, cx| {
                                view.display_menu = false;
                                cx.notify();
                            }),
                        ),
                )
                .child(reveal(panel, "display-reveal")),
        )
    }
}
