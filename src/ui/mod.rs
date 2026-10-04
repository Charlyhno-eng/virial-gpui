mod components;
mod context_menu;
mod file_list;
pub(crate) mod input;
mod sidebar;
mod titlebar;
mod toolbar;

use crate::{app::FileManager, theme::*};
use gpui::{Context, Render, Window, div, prelude::*, px};

impl Render for FileManager {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_window_title(&format!(
            "{} — Virial",
            self.location.description(self.language)
        ));
        div()
            .id("file-manager")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .on_mouse_move(cx.listener(Self::move_window))
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(color(BACKGROUND))
            .text_color(color(TEXT))
            .text_size(px(12.))
            .font_family("sans-serif")
            .child(self.titlebar(window, cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.sidebar(cx))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .child(self.toolbar(cx))
                            .child(self.file_list(cx)),
                    ),
            )
            .children(titlebar::resize_handles(window))
            .children(self.context_overlay(window, cx))
    }
}
