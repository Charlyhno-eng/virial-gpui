use crate::{app::FileManager, ui::icons::icon, ui::theme::*};
use gpui::{Animation, AnimationExt, Context, Div, FontWeight, div, prelude::*, px, uniform_list};
use std::time::Duration;

impl FileManager {
    pub(crate) fn file_list(&self, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let title = self.location.title(&self.home, self.language);
        let scroll = self.scroll.0.borrow().base_handle.clone();
        let rectangle = self
            .marquee
            .as_ref()
            .map(|marquee| (marquee.start, marquee.end));
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
            .when_some(self.location.directory().map(|path| path.to_path_buf()), |area, directory| {
                self.drop_target(area, directory, cx)
            })
            .min_w_0()
            .min_h_0()
            .child(
                div()
                    .px_4()
                    .pt_3()
                    .pb_3()
                    .flex_shrink_0()
                    .child(
                        div()
                            .text_size(px(17.))
                            .text_ellipsis()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(11.))
                            .text_color(color(MUTED))
                            .text_ellipsis()
                            .child(self.location.description(self.language)),
                    ),
            )
            .children(self.error.as_ref().map(|error| {
                div()
                    .mx_4()
                    .mb_3()
                    .p_3()
                    .rounded_md()
                    .bg(color(ERROR_BG))
                    .text_size(px(11.))
                    .text_color(color(ERROR))
                    .child(error.clone())
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(28.))
                    .flex_shrink_0()
                    .gap_3()
                    .px_4()
                    .border_b_1()
                    .border_color(color(BORDER))
                    .text_size(px(10.))
                    .text_color(color(MUTED))
                    .child(div().flex_1().min_w_0().child(self.language.text("NAME")))
                    .child(div().w(px(104.)).child(self.language.text("KIND")))
                    .child(
                        div()
                            .w(px(76.))
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
                } else if self.location == crate::domain::location::Location::Recent {
                    (
                        "No recent files",
                        "Files opened with Virial and desktop applications appear here",
                    )
                } else if self.location == crate::domain::location::Location::Network {
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
                    .min_h_0()
                    .px_4()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .text_center()
                    .gap_3()
                    .child(icon(self.location.icon(), 40., ACCENT))
                    .child(div().text_size(px(13.)).child(self.language.text(heading)))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(color(MUTED))
                            .child(self.language.text(description)),
                    )
                    .into_any_element()
            } else {
                div().relative().flex().flex_col().flex_1().min_h_0().overflow_hidden()
                .child(uniform_list(
                    self.location.id(),
                    self.entries.len(),
                    cx.processor(|view, range: std::ops::Range<usize>, _, cx| {
                        let paths = view.selected_paths().into();
                        range
                            .map(|index| {
                                let entry = view.entries[index].clone();
                                let selected = view.selection.indices.contains(&index);
                                let payload = view.drag_payload(index, &paths);
                                let directory = entry.directory.then(|| entry.path.clone());
                                let row = div()
                                    .id(std::sync::Arc::<std::path::Path>::from(entry.path.clone()))
                                    .flex_1()
                                    .min_w_0()
                                    .h(px(ROW_HEIGHT))
                                    .px_4()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .cursor_pointer()
                                    .border_l_2()
                                    .border_color(if selected {
                                        color(ACCENT)
                                    } else {
                                        gpui::transparent_black()
                                    })
                                    .when(selected, |row| row.bg(color(SELECTED)))
                                    .when(!selected && index % 2 != 0, |row| {
                                        row.bg(translucent(SIDEBAR, 0.22))
                                    })
                                    .hover(|style| {
                                        style.bg(color(if selected { SELECTED } else { HOVER }))
                                    })
                                    .child(icon(
                                        entry.icon(),
                                        19.,
                                        if entry.directory { ACCENT_BLUE } else { MUTED },
                                    ))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .text_ellipsis()
                                            .text_size(px(11.))
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
                                            .w(px(104.))
                                            .flex_shrink_0()
                                            .text_size(px(11.))
                                            .text_color(color(MUTED))
                                            .child(view.language.text(entry.kind())),
                                    )
                                    .child(
                                        div()
                                            .w(px(76.))
                                            .flex_shrink_0()
                                            .text_right()
                                            .text_size(px(11.))
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
                                    .on_mouse_down(gpui::MouseButton::Left, cx.listener(
                                        move |view, event: &gpui::MouseDownEvent, window, cx| {
                                            if view.busy || view.loading { return; }
                                            view.focus.focus(window);
                                            // Defer Ctrl toggles until release so Ctrl-drag
                                            // can copy an existing multiple selection.
                                            if event.modifiers.shift || (!event.modifiers.control
                                                && !view.selection.indices.contains(&index))
                                            {
                                                view.selection.click(index, event.modifiers.control, event.modifiers.shift);
                                            }
                                            cx.notify();
                                        },
                                    ))
                                    .when(!view.busy && !view.loading && view.marquee.is_none(), |row| {
                                        row.on_drag(payload, |drag, _, _, cx| {
                                            cx.new(|_| drag.clone())
                                        })
                                        .on_click(cx.listener(
                                            move |view, event: &gpui::ClickEvent, _, cx| {
                                                if !event.standard_click() { return; }
                                                let modifiers = event.modifiers();
                                                if modifiers.control && !modifiers.shift {
                                                    view.selection.click(index, true, false);
                                                } else if !modifiers.control && !modifiers.shift {
                                                    view.selection.click(index, false, false);
                                                    if event.click_count() >= 2 {
                                                        view.open(entry.clone(), cx);
                                                    }
                                                }
                                                cx.notify();
                                            },
                                        ))
                                    });
                                let row = if let Some(directory) = directory {
                                    view.drop_target(row, directory, cx)
                                } else { row };
                                div().w_full().h(px(ROW_HEIGHT)).flex()
                                    .child(div().w(px(14.)).h_full().flex_shrink_0())
                                    .child(row)
                            })
                            .collect()
                    }),
                )
                .on_mouse_down(gpui::MouseButton::Left, cx.listener(Self::begin_marquee))
                .w_full()
                .track_scroll(self.scroll.clone())
                .flex_1()
                .min_h_0())
                .children(rectangle.map(|(start, end)| {
                    gpui::canvas(move |_, _, _| {
                        let offset = scroll.offset();
                        let bounds = scroll.bounds();
                        let top_left = gpui::point(start.x.min(end.x), start.y.min(end.y)) + bounds.origin + offset;
                        let size = gpui::size((end.x - start.x).abs(), (end.y - start.y).abs());
                        gpui::Bounds::new(top_left, size)
                    }, |_, bounds, window, _| {
                        window.paint_quad(gpui::quad(bounds, px(0.), translucent(ACCENT_BLUE, 0.15), px(1.), color(ACCENT_BLUE), gpui::BorderStyle::Solid));
                    }).absolute().size_full()
                }))
                .into_any_element()
            })
            .child(
                div()
                    .h(px(30.))
                    .flex_shrink_0()
                    .px_4()
                    .flex()
                    .items_center()
                    .gap_2()
                    .border_t_1()
                    .border_color(color(BORDER))
                    .text_size(px(10.))
                    .text_color(color(MUTED))
                    .when(self.busy || self.loading, |bar| {
                        bar.child(
                            div()
                                .size(px(5.))
                                .rounded_full()
                                .bg(color(ACCENT_BLUE))
                                .with_animation(
                                    "activity",
                                    Animation::new(Duration::from_millis(1200))
                                        .repeat()
                                        .with_easing(gpui::pulsating_between(0.4, 1.)),
                                    |dot, delta| dot.opacity(delta),
                                ),
                        )
                    })
                    .child(if self.busy {
                        self.language.text("Working…").into()
                    } else if self.loading {
                        self.language.text("Loading…").into()
                    } else {
                        self.language
                            .counts(self.folder_count, self.entries.len() - self.folder_count)
                    })
                    .child(div().flex_1().min_w_0().text_ellipsis().text_color(color(TEXT))
                        .child(if self.selection.indices.len() > 1 {
                            format!(" · {}", self.language.selected_count(self.selection.indices.len()))
                        } else {
                            self.selection.primary().and_then(|index| self.entries.get(index))
                                .map(|entry| format!(" · {}", entry.name)).unwrap_or_default()
                        }))
                    .child(self.language.text("Double-click to open · Enter")),
            )
    }
}
