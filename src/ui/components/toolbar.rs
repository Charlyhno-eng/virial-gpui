use crate::ui::components::{navigation_button, toolbar_button};
use crate::{app::FileManager, ui::icons::icon, ui::theme::*};
use gpui::{Context, Div, Window, div, prelude::*, px};

impl FileManager {
    pub(crate) fn toolbar(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let filter_label = self.language.text("Filter files");
        let search_label = self.language.text("Search everywhere · Ctrl+P");
        let breadcrumbs = self.location.breadcrumbs(&self.home, self.language);
        let hidden_count = breadcrumbs.len().saturating_sub(2);
        let navigation = div()
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
                div()
                    .id("breadcrumbs")
                    .flex()
                    .items_center()
                    .flex_1()
                    .min_w(px(140.))
                    .overflow_x_scroll()
                    .when(hidden_count > 0, |crumbs| {
                        crumbs.child(
                            div()
                                .flex_shrink_0()
                                .px_2()
                                .py_1()
                                .text_size(px(11.))
                                .text_color(color(MUTED))
                                .child("…"),
                        )
                    })
                    .children(breadcrumbs.into_iter().enumerate().skip(hidden_count).map(
                        |(index, (label, path))| {
                            let current = path == self.location;
                            div()
                                .flex()
                                .items_center()
                                .flex_shrink_0()
                                .when(index > 0, |crumb| crumb.child(icon("forward", 13., MUTED)))
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
                        },
                    )),
            )
            .when(
                self.location != crate::domain::location::Location::Workspaces,
                |bar| {
                    bar.child(
                        toolbar_button(
                            "toggle-filter",
                            "filter",
                            "",
                            true,
                            self.filter_open || !self.extension_filter.is_empty(),
                        )
                        .tooltip(move |_, cx| {
                            cx.new(|_| super::modal::StatusTooltip(filter_label.into()))
                                .into()
                        })
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.filter_open = !view.filter_open;
                            if view.filter_open {
                                view.extension_input.read(cx).focus(window);
                            } else {
                                view.focus.focus(window);
                            }
                            cx.notify();
                        })),
                    )
                },
            )
            .when(
                self.location == crate::domain::location::Location::Trash,
                |bar| {
                    bar.child(
                        toolbar_button(
                            "restore-trash",
                            "back",
                            self.language.text("Restore"),
                            !self.busy && !self.selection.indices.is_empty(),
                            true,
                        )
                        .on_click(cx.listener(|view, _, _, cx| view.restore_trash(cx))),
                    )
                },
            )
            .when(
                self.location == crate::domain::location::Location::Trash,
                |bar| {
                    bar.child(
                        toolbar_button(
                            "empty-trash",
                            "trash",
                            self.language.text("Empty Trash…"),
                            !self.busy && !self.loading,
                            false,
                        )
                        .on_click(
                            cx.listener(|view, _, window, cx| view.request_empty_trash(window, cx)),
                        ),
                    )
                },
            )
            .child(
                toolbar_button("toggle-search", "search", "", true, self.search_open)
                    .tooltip(move |_, cx| {
                        cx.new(|_| super::modal::StatusTooltip(search_label.into()))
                            .into()
                    })
                    .on_click(
                        cx.listener(|view, _, window, cx| view.toggle_global_search(window, cx)),
                    ),
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
            });
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .child(navigation)
            .when(
                self.search_open
                    || (self.filter_open
                        && self.location != crate::domain::location::Location::Workspaces),
                |bar| {
                    bar.child(
                        div()
                            .flex()
                            .justify_end()
                            .items_start()
                            .h(px(60.))
                            .gap_3()
                            .px_4()
                            .py_2()
                            .bg(translucent(SURFACE, 0.2))
                            .border_b_1()
                            .border_color(color(BORDER))
                            .when(
                                self.filter_open
                                    && self.location
                                        != crate::domain::location::Location::Workspaces,
                                |row| {
                                    row.child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .w(px(230.))
                                            .min_w_0()
                                            .child(
                                                div()
                                                    .text_size(px(10.))
                                                    .text_color(color(MUTED))
                                                    .child(self.language.text("Filter files")),
                                            )
                                            .child(self.extension_input.clone()),
                                    )
                                },
                            )
                            .when(self.search_open, |row| {
                                row.child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .w(px(280.))
                                        .min_w_0()
                                        .child(
                                            div()
                                                .text_size(px(10.))
                                                .text_color(color(MUTED))
                                                .child(self.language.text("Search everywhere")),
                                        )
                                        .child(self.search_input.clone()),
                                )
                            }),
                    )
                },
            )
    }
}
