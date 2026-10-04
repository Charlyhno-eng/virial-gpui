use super::components::section_label;
use crate::{app::FileManager, icons::icon, theme::*};
use gpui::{Context, Div, div, prelude::*, px};
use std::path::PathBuf;

impl FileManager {
    pub(super) fn sidebar(&self, cx: &mut Context<Self>) -> Div {
        div()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .bg(color(SIDEBAR))
            .border_r_1()
            .border_color(color(BORDER))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_5()
                    .h(px(72.))
                    .child(
                        div()
                            .size(px(32.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_lg()
                            .bg(color(SELECTED))
                            .child(icon("folder", 21., ACCENT)),
                    )
                    .child(
                        div()
                            .text_size(px(20.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Virial"),
                    ),
            )
            .child(
                div()
                    .id("places")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_2()
                    .child(section_label("PLACES"))
                    .children(
                        self.places
                            .iter()
                            .take(self.places.len() - 1)
                            .enumerate()
                            .map(|(index, place)| {
                                self.place(index, place.label, place.icon, place.path.clone(), cx)
                            }),
                    )
                    .child(section_label("DEVICES"))
                    .child(self.place(100, "File System", "drive", PathBuf::from("/"), cx)),
            )
            .child(
                div()
                    .px_5()
                    .py_4()
                    .border_t_1()
                    .border_color(color(BORDER))
                    .text_size(px(11.))
                    .text_color(color(MUTED))
                    .child("LOCAL FILES"),
            )
    }

    fn place(
        &self,
        index: usize,
        label: &'static str,
        symbol: &'static str,
        path: PathBuf,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let active = self
            .places
            .iter()
            .filter(|place| self.path.starts_with(&place.path))
            .max_by_key(|place| place.path.components().count())
            .is_some_and(|place| place.path == path);
        div()
            .id(("place", index))
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .h(px(38.))
            .mb_1()
            .rounded_md()
            .cursor_pointer()
            .text_size(px(13.))
            .text_color(color(if active { ACCENT } else { TEXT }))
            .when(active, |row| row.bg(color(SELECTED)))
            .hover(|style| style.bg(color(if active { SELECTED } else { HOVER })))
            .child(icon(symbol, 18., if active { ACCENT } else { MUTED }))
            .child(label)
            .on_click(cx.listener(move |view, _, window, cx| {
                view.focus.focus(window);
                view.navigate(path.clone(), cx);
            }))
    }
}
