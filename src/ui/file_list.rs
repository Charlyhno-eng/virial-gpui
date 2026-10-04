use crate::{app::FileManager, files::format_size, icons::icon, theme::*};
use gpui::{Context, Div, FontWeight, div, prelude::*, px, uniform_list};

impl FileManager {
    pub(super) fn file_list(&self, cx: &mut Context<Self>) -> Div {
        let title = if self.path == self.home {
            "Home".to_string()
        } else {
            self.path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "File System".into())
        };
        let folders = self.entries.iter().filter(|entry| entry.directory).count();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(
                div()
                    .px_6()
                    .pt_5()
                    .pb_4()
                    .child(
                        div()
                            .text_size(px(23.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(12.))
                            .text_color(color(MUTED))
                            .child(self.path.display().to_string()),
                    ),
            )
            .children(self.error.as_ref().map(|error| {
                div()
                    .mx_6()
                    .mb_3()
                    .p_3()
                    .rounded_md()
                    .bg(color(ERROR_BG))
                    .text_size(px(13.))
                    .text_color(color(ERROR))
                    .child(error.clone())
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(34.))
                    .gap_3()
                    .px_6()
                    .border_b_1()
                    .border_color(color(BORDER))
                    .text_size(px(11.))
                    .text_color(color(MUTED))
                    .child(div().flex_1().min_w_0().child("NAME"))
                    .child(div().w(px(130.)).child("KIND"))
                    .child(div().w(px(100.)).text_right().child("SIZE")),
            )
            .child(if self.entries.is_empty() {
                let (heading, description) = if self.loading {
                    ("Loading folder…", "Reading directory contents")
                } else if self.error.is_some() {
                    (
                        "Folder unavailable",
                        "Choose another location or try refreshing",
                    )
                } else {
                    (
                        "This folder is empty",
                        "Hidden files can be shown from the toolbar",
                    )
                };
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .child(icon("folder", 52., MUTED))
                    .child(div().text_size(px(17.)).child(heading))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(color(MUTED))
                            .child(description),
                    )
                    .into_any_element()
            } else {
                uniform_list(
                    std::sync::Arc::<std::path::Path>::from(self.path.clone()),
                    self.entries.len(),
                    cx.processor(|view, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|index| {
                                let entry = view.entries[index].clone();
                                let selected = view.selected == Some(index);
                                div()
                                    .id(std::sync::Arc::<std::path::Path>::from(entry.path.clone()))
                                    .w_full()
                                    .h(px(ROW_HEIGHT))
                                    .px_6()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .cursor_pointer()
                                    .border_b_1()
                                    .border_color(color(BACKGROUND))
                                    .bg(color(if selected {
                                        SELECTED
                                    } else if index % 2 == 0 {
                                        BACKGROUND
                                    } else {
                                        SIDEBAR
                                    }))
                                    .hover(|style| {
                                        style.bg(color(if selected { SELECTED } else { HOVER }))
                                    })
                                    .child(icon(
                                        entry.icon(),
                                        23.,
                                        if entry.directory { ACCENT } else { MUTED },
                                    ))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .text_ellipsis()
                                            .text_size(px(14.))
                                            .child(entry.name.clone()),
                                    )
                                    .child(
                                        div()
                                            .w(px(130.))
                                            .flex_shrink_0()
                                            .text_size(px(12.))
                                            .text_color(color(MUTED))
                                            .child(entry.kind()),
                                    )
                                    .child(
                                        div()
                                            .w(px(100.))
                                            .flex_shrink_0()
                                            .text_right()
                                            .text_size(px(12.))
                                            .text_color(color(MUTED))
                                            .child(format_size(entry.bytes)),
                                    )
                                    .on_click(cx.listener(
                                        move |view, event: &gpui::ClickEvent, window, cx| {
                                            if !event.standard_click() {
                                                return;
                                            }
                                            view.focus.focus(window);
                                            view.selected = Some(index);
                                            cx.notify();
                                            if event.click_count() >= 2 {
                                                view.open(entry.clone(), cx);
                                            }
                                        },
                                    ))
                            })
                            .collect()
                    }),
                )
                .w_full()
                .track_scroll(self.scroll.clone())
                .flex_1()
                .min_h_0()
                .into_any_element()
            })
            .child(
                div()
                    .h(px(38.))
                    .px_6()
                    .flex()
                    .items_center()
                    .gap_2()
                    .border_t_1()
                    .border_color(color(BORDER))
                    .text_size(px(12.))
                    .text_color(color(MUTED))
                    .child(if self.loading {
                        "Loading…".into()
                    } else {
                        format!("{folders} folders · {} files", self.entries.len() - folders)
                    })
                    .children(self.selected.and_then(|index| self.entries.get(index)).map(
                        |entry| {
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_ellipsis()
                                .text_color(color(TEXT))
                                .child(format!(" · {}", entry.name))
                        },
                    ))
                    .when(self.selected.is_none(), |bar| bar.child(div().flex_1()))
                    .child("Double-click to open · Enter"),
            )
    }
}
