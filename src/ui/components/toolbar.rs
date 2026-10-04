use crate::ui::components::toolbar_button;
use crate::{app::FileManager, ui::icons::icon, ui::theme::*};
use gpui::{Context, Div, Window, div, prelude::*, px};

impl FileManager {
    pub(crate) fn toolbar(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .bg(translucent(SURFACE, 0.35))
            .border_b_1()
            .border_color(color(BORDER))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_1()
                    .px_4()
                    .py_2()
                    .child(
                        toolbar_button(
                            "back",
                            "back",
                            self.language.text("Back"),
                            self.history.back().is_some(),
                            false,
                        )
                        .on_click(cx.listener(|view, _, _, cx| view.back(cx))),
                    )
                    .child(
                        toolbar_button(
                            "forward",
                            "forward",
                            self.language.text("Forward"),
                            self.history.forward().is_some(),
                            false,
                        )
                        .on_click(cx.listener(|view, _, _, cx| view.forward(cx))),
                    )
                    .child(
                        toolbar_button(
                            "up",
                            "up",
                            self.language.text("Up"),
                            self.location
                                .directory()
                                .and_then(|path| path.parent())
                                .is_some(),
                            false,
                        )
                        .on_click(cx.listener(|view, _, _, cx| view.up(cx))),
                    )
                    .child(div().w(px(1.)).h(px(20.)).mx_2().bg(color(BORDER)))
                    .child(
                        toolbar_button(
                            "open",
                            "open",
                            self.language.text("Open"),
                            self.selected.is_some(),
                            false,
                        )
                        .on_click(cx.listener(|view, _, _, cx| view.open_selected(cx))),
                    )
                    .child(div().flex_1())
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
                            "hidden",
                            "eye",
                            self.language.text("Hidden files"),
                            true,
                            self.hidden,
                        )
                        .on_click(cx.listener(|view, _, _, cx| view.toggle_hidden(cx))),
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
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_4()
                    .pb_2()
                    .child(icon(self.location.icon(), 16., ACCENT_BLUE))
                    .child(
                        div()
                            .id("breadcrumbs")
                            .flex()
                            .items_center()
                            .flex_1()
                            .min_w_0()
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
                                                    .px_2()
                                                    .py_1()
                                                    .rounded_sm()
                                                    .cursor_pointer()
                                                    .text_size(px(11.))
                                                    .text_color(color(if current {
                                                        TEXT
                                                    } else {
                                                        MUTED
                                                    }))
                                                    .hover(|style| {
                                                        style
                                                            .bg(color(HOVER))
                                                            .text_color(color(TEXT))
                                                    })
                                                    .child(label)
                                                    .on_click(cx.listener(
                                                        move |view, _, _, cx| {
                                                            view.navigate_location(path.clone(), cx)
                                                        },
                                                    )),
                                            )
                                    }),
                            ),
                    ),
            )
    }
}
