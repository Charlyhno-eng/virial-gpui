use crate::ui::components::{navigation_button, toolbar_button};
use crate::{app::FileManager, ui::icons::icon, ui::theme::*};
use gpui::{Context, Div, Window, div, prelude::*, px};

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
            .when(
                self.location != crate::domain::location::Location::Workspaces,
                |bar| {
                    bar.child(
                        div()
                            .id("extension-filter")
                            .w(px(150.))
                            .max_w_full()
                            .child(self.extension_input.clone()),
                    )
                },
            )
            .child(
                div()
                    .id("search-field")
                    .w(px(200.))
                    .max_w_full()
                    .child(self.search_input.clone()),
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
}
