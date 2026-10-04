mod components;
mod file_list;
mod sidebar;
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
            .size_full()
            .flex()
            .bg(color(BACKGROUND))
            .text_color(color(TEXT))
            .text_size(px(14.))
            .font_family("sans-serif")
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
            )
    }
}
