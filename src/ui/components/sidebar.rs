use crate::ui::components::section_label;
use crate::{app::FileManager, domain::location::Location, ui::icons::icon, ui::theme::*};
use gpui::{Context, Div, div, prelude::*, px};
use std::path::PathBuf;

impl FileManager {
    pub(crate) fn sidebar(&self, cx: &mut Context<Self>) -> Div {
        div()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .bg(translucent(SIDEBAR, 0.45))
            .border_r_1()
            .border_color(color(BORDER))
            .child(
                div()
                    .id("places")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_2()
                    .child(section_label(self.language.text("PLACES")))
                    .children(
                        self.places
                            .iter()
                            .take(self.places.len() - 1)
                            .enumerate()
                            .map(|(index, place)| {
                                self.place(
                                    index,
                                    place.label,
                                    place.icon,
                                    place.path.clone().into(),
                                    cx,
                                )
                            }),
                    )
                    .child(self.place(101, "Recent", "recent", Location::Recent, cx))
                    .child(self.place(102, "Network", "network", Location::Network, cx))
                    .child(section_label(self.language.text("DEVICES")))
                    .child(self.place(100, "File System", "drive", PathBuf::from("/").into(), cx)),
            )
            .child(
                div()
                    .px_4()
                    .h(px(30.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .border_t_1()
                    .border_color(color(BORDER))
                    .text_size(px(10.))
                    .text_color(color(MUTED))
                    .child(self.language.text("LOCAL FILES")),
            )
    }

    fn place(
        &self,
        index: usize,
        label: &'static str,
        symbol: &'static str,
        location: Location,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let active = if let Some(path) = location.directory() {
            self.location.directory().is_some_and(|current| {
                self.places
                    .iter()
                    .filter(|place| current.starts_with(&place.path))
                    .max_by_key(|place| place.path.components().count())
                    .is_some_and(|place| place.path == path)
            })
        } else {
            self.location == location
        };
        div()
            .id(("place", index))
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .h(px(30.))
            .mb_1()
            .rounded_md()
            .cursor_pointer()
            .text_size(px(11.))
            .text_color(color(if active { ACCENT } else { TEXT }))
            .when(active, |row| row.bg(color(SELECTED)))
            .hover(|style| style.bg(color(if active { SELECTED } else { HOVER })))
            .active(|style| style.bg(color(SELECTED)))
            .child(icon(symbol, 16., if active { ACCENT } else { MUTED }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_ellipsis()
                    .child(self.language.text(label)),
            )
            .when(active, |row| {
                row.child(div().w(px(3.)).h(px(12.)).rounded_full().bg(color(ACCENT)))
            })
            .on_click(cx.listener(move |view, _, window, cx| {
                view.focus.focus(window);
                view.navigate_location(location.clone(), cx);
            }))
    }
}
