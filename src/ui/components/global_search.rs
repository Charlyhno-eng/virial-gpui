use crate::{
    app::FileManager,
    ui::{icons::file_icon, theme::*},
};
use gpui::{Context, Div, Window, div, prelude::*, px, uniform_list};

impl FileManager {
    pub(crate) fn global_search_overlay(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let picker = self.global_search.as_ref()?;
        let status = if picker.query.is_empty() {
            "Type a name or path; spaces separate search terms"
        } else if !picker.results.finished {
            "Searching…"
        } else if picker.results.entries.is_empty() {
            "No matches found"
        } else {
            "Best 100 matches · ↑/↓ select · Enter opens · Escape closes"
        };
        let top = if window.is_fullscreen() { 42. } else { 74. };
        let available = (f32::from(window.viewport_size().height) - top - 8.).max(100.);
        Some(
            div().absolute().right_3().top(px(top)).max_w_full().child(
                div()
                    .id("global-search-panel")
                    .occlude()
                    .w(px(560.))
                    .max_w_full()
                    .max_h(px(available))
                    .p_2()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .rounded_lg()
                    .bg(color(SURFACE))
                    .border_1()
                    .border_color(color(BORDER))
                    .shadow_lg()
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .child(self.language.text("Search everywhere")),
                            )
                            .child(
                                div()
                                    .id("close-global-search")
                                    .cursor_pointer()
                                    .child(self.language.text("Close"))
                                    .on_click(cx.listener(|view, _, window, cx| {
                                        view.close_global_search(window, cx)
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .text_color(color(MUTED))
                            .child(self.language.text(status)),
                    )
                    .when(picker.results.skipped > 0, |panel| {
                        panel.child(div().text_color(color(MUTED)).child(format!(
                            "{}: {}",
                            self.language.text("Unreadable locations skipped"),
                            picker.results.skipped
                        )))
                    })
                    .child(
                        uniform_list(
                            "global-search-results",
                            picker.results.entries.len(),
                            cx.processor(|view, range: std::ops::Range<usize>, _, cx| {
                                let Some(picker) = view.global_search.as_ref() else {
                                    return Vec::new();
                                };
                                range
                                    .map(|index| {
                                        let entry = &picker.results.entries[index];
                                        div()
                                            .id(("global-search-result", index))
                                            .h(px(48.))
                                            .px_2()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .rounded_md()
                                            .cursor_pointer()
                                            .bg(color(if picker.selected == index {
                                                SELECTED
                                            } else {
                                                SURFACE
                                            }))
                                            .hover(|style| style.bg(color(HOVER)))
                                            .child(file_icon(entry.icon(), 16.))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .flex()
                                                    .flex_col()
                                                    .child(
                                                        div()
                                                            .text_ellipsis()
                                                            .child(entry.name.clone()),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_ellipsis()
                                                            .text_color(color(MUTED))
                                                            .child(
                                                                entry
                                                                    .path
                                                                    .to_string_lossy()
                                                                    .into_owned(),
                                                            ),
                                                    ),
                                            )
                                            .on_click(cx.listener(
                                                move |view, _, window: &mut Window, cx| {
                                                    view.open_global_result(index, window, cx)
                                                },
                                            ))
                                    })
                                    .collect()
                            }),
                        )
                        .track_scroll(picker.scroll.clone())
                        .h(px(280.))
                        .min_h_0()
                        .max_h(px((available - 90.).max(40.))),
                    ),
            ),
        )
    }
}
