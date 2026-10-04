use crate::{app::FileManager, icons::icon, theme::*};
use gpui::{Context, Div, FontWeight, div, prelude::*, px, uniform_list};

impl FileManager {
    pub(super) fn file_list(&self, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let title = self.location.title(&self.home, self.language);
        let folders = self.entries.iter().filter(|entry| entry.directory).count();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .id("file-list-area")
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(|view, event: &gpui::MouseDownEvent, window, cx| {
                    view.show_menu(None, event.position, window, cx);
                }),
            )
            .min_w_0()
            .h_full()
            .child(
                div()
                    .px_6()
                    .pt_5()
                    .pb_4()
                    .child(
                        div()
                            .text_size(px(20.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(12.))
                            .text_color(color(MUTED))
                            .child(self.location.description(self.language)),
                    ),
            )
            .children(self.error.as_ref().map(|error| {
                div()
                    .mx_6()
                    .mb_3()
                    .p_3()
                    .rounded_md()
                    .bg(color(ERROR_BG))
                    .text_size(px(12.))
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
                    .child(div().flex_1().min_w_0().child(self.language.text("NAME")))
                    .child(div().w(px(130.)).child(self.language.text("KIND")))
                    .child(
                        div()
                            .w(px(100.))
                            .text_right()
                            .child(self.language.text("SIZE")),
                    ),
            )
            .child(if self.entries.is_empty() {
                let (heading, description) = if self.loading {
                    ("Loading folder…", "Reading directory contents")
                } else if self.error.is_some() {
                    (
                        "Folder unavailable",
                        "Choose another location or try refreshing",
                    )
                } else if self.location == crate::location::Location::Recent {
                    (
                        "No recent files",
                        "Files opened with Virial and desktop applications appear here",
                    )
                } else if self.location == crate::location::Location::Network {
                    (
                        "No mounted network locations",
                        "Mount a network share using your desktop, then refresh",
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
                    .child(icon(self.location.icon(), 52., MUTED))
                    .child(div().text_size(px(15.)).child(self.language.text(heading)))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(color(MUTED))
                            .child(self.language.text(description)),
                    )
                    .into_any_element()
            } else {
                uniform_list(
                    self.location.id(),
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
                                            .text_size(px(12.))
                                            .child(entry.name.clone())
                                            .when(view.location.directory().is_none(), |name| {
                                                name.child(
                                                    div()
                                                        .text_size(px(10.))
                                                        .text_color(color(MUTED))
                                                        .text_ellipsis()
                                                        .child(entry.path.display().to_string()),
                                                )
                                            }),
                                    )
                                    .child(
                                        div()
                                            .w(px(130.))
                                            .flex_shrink_0()
                                            .text_size(px(12.))
                                            .text_color(color(MUTED))
                                            .child(view.language.text(entry.kind())),
                                    )
                                    .child(
                                        div()
                                            .w(px(100.))
                                            .flex_shrink_0()
                                            .text_right()
                                            .text_size(px(12.))
                                            .text_color(color(MUTED))
                                            .child(view.language.size(entry.bytes)),
                                    )
                                    .on_mouse_down(gpui::MouseButton::Right, {
                                        let entry = entry.clone();
                                        cx.listener(
                                            move |view,
                                                  event: &gpui::MouseDownEvent,
                                                  window,
                                                  cx| {
                                                cx.stop_propagation();
                                                view.show_menu(
                                                    Some(entry.clone()),
                                                    event.position,
                                                    window,
                                                    cx,
                                                );
                                            },
                                        )
                                    })
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
                    .child(if self.busy {
                        self.language.text("Working…").into()
                    } else if self.loading {
                        self.language.text("Loading…").into()
                    } else {
                        self.language.counts(folders, self.entries.len() - folders)
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
                    .child(self.language.text("Double-click to open · Enter")),
            )
    }
}
