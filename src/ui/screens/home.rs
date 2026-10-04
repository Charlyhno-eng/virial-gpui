use crate::{app::FileManager, ui::icons::icon, ui::theme::*};
use gpui::{Animation, AnimationExt, Context, Div, FontWeight, div, prelude::*, px, uniform_list};
use std::time::Duration;

impl FileManager {
    pub(crate) fn details_panel(&self, current_folder: bool, _: &mut Context<Self>) -> Div {
        let selected = (!current_folder)
            .then(|| {
                self.selection
                    .primary()
                    .and_then(|index| self.entries.get(index))
                    .cloned()
            })
            .flatten();
        let has_selection = selected.is_some();
        let modified = selected
            .as_ref()
            .map(|entry| entry.path.as_path())
            .or_else(|| self.location.directory())
            .and_then(|path| std::fs::metadata(path).ok())
            .and_then(|metadata| metadata.modified().ok())
            .map(|modified| {
                chrono::DateTime::<chrono::Utc>::from(modified)
                    .format("%Y-%m-%d %H:%M UTC")
                    .to_string()
            })
            .unwrap_or_else(|| "—".to_string());
        let (name, path, kind, size, symbol) = if let Some(entry) = selected {
            let kind = entry.kind();
            let symbol = entry.icon();
            (
                entry.name,
                entry.path.display().to_string(),
                kind,
                self.language.size(entry.bytes),
                symbol,
            )
        } else {
            let name = self.location.title(&self.home, self.language);
            let path = self.location.description(self.language);
            let kind = if self.location.directory().is_some() {
                "Folder"
            } else {
                "Location"
            };
            let size = "—".to_string();
            (name, path, kind, size, self.location.icon())
        };
        div()
            .w_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_3()
            .px_4()
            .pt_5()
            .pb_4()
            .bg(translucent(SURFACE, 0.16))
            .child(icon(symbol, 34., ACCENT_BLUE))
            .child(div().text_size(px(14.)).text_ellipsis().child(name))
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(color(MUTED))
                    .text_ellipsis()
                    .child(path),
            )
            .child(div().h(px(1.)).my_1().bg(color(BORDER)))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .text_size(px(11.))
                    .child(self.language.text("Type"))
                    .child(
                        div()
                            .text_color(color(MUTED))
                            .child(self.language.text(kind)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .text_size(px(11.))
                    .child(self.language.text("Size"))
                    .child(div().text_color(color(MUTED)).child(size)),
            )
            .when(
                current_folder || matches!(self.preview, crate::state::preview::Preview::Folder(_)),
                |panel| {
                    let count = if current_folder {
                        self.entries.len()
                    } else if let crate::state::preview::Preview::Folder(count) = self.preview {
                        count
                    } else {
                        0
                    };
                    panel.child(
                        div()
                            .flex()
                            .justify_between()
                            .text_size(px(11.))
                            .child(self.language.text("Contents"))
                            .child(
                                div()
                                    .text_color(color(MUTED))
                                    .child(self.language.item_count(count)),
                            ),
                    )
                },
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .text_size(px(11.))
                    .child(self.language.text("Modified"))
                    .child(div().text_color(color(MUTED)).child(modified)),
            )
            .child(div().h(px(1.)).my_1().bg(color(BORDER)))
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(color(MUTED))
                    .child(self.language.text("DETAILS")),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(color(MUTED))
                    .child(self.language.text(if has_selection {
                        "Selected item"
                    } else {
                        "Current location"
                    })),
            )
    }

    pub(crate) fn file_list(&self, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let title = self.location.title(&self.home, self.language);
        let query = self.search_input.read(cx).text.trim().to_lowercase();
        let visible_entries: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| query.is_empty() || entry.name.to_lowercase().contains(&query))
            .map(|(index, _)| index)
            .collect();
        let visible_count = visible_entries.len();
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
                            .flex().items_center().gap_2()
                            .child(icon(self.location.icon(), 26., ACCENT_BLUE))
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
                    .child(div().id("sort-name").flex_1().min_w_0().flex().items_center().gap_2().cursor_pointer()
                        .hover(|style| style.text_color(color(TEXT)))
                        .child(self.language.text("NAME"))
                        .child(icon(if self.name_descending { "down" } else { "up" }, 11., MUTED))
                        .on_click(cx.listener(|view, _, _, cx| view.toggle_name_sort(cx))))
                    .child(div().w(px(104.)).child(self.language.text("KIND")))
                    .child(
                        div()
                            .w(px(76.))
                            .text_right()
                            .child(self.language.text("SIZE")),
                    ),
            )
            .child(if visible_entries.is_empty() {
                let (heading, description) = if self.loading {
                    ("Loading folder…", "Reading directory contents")
                } else if self.error.is_some() {
                    (
                        "Folder unavailable",
                        "Choose another location or try refreshing",
                    )
                } else if !query.is_empty() {
                    ("No matching items", "Try a different name")
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
                    visible_count,
                    cx.processor(|view, range: std::ops::Range<usize>, _, cx| {
                        let query = view.search_input.read(cx).text.trim().to_lowercase();
                        let visible: Vec<usize> = view.entries.iter().enumerate()
                            .filter(|(_, entry)| query.is_empty() || entry.name.to_lowercase().contains(&query))
                            .map(|(index, _)| index).collect();
                        let paths = view.selected_paths().into();
                        range
                            .map(|visible_index| {
                                let index = visible[visible_index];
                                let entry = view.entries[index].clone();
                                let selected = view.selection.indices.contains(&index);
                                let payload = view.drag_payload(index, &paths);
                                let directory = entry.directory.then(|| entry.path.clone());
                                let row = div()
                                    .id(std::sync::Arc::<std::path::Path>::from(entry.path.clone()))
                                    .flex_1()
                                    .min_w_0()
                                    .h(px(if view.compact_view { ROW_HEIGHT * 0.78 } else { ROW_HEIGHT }))
                                    .px_4()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .cursor_pointer()
                                    .border_l_2()
                                    .border_color(if selected {
                                        translucent(ACCENT_BLUE, 0.28)
                                    } else {
                                        gpui::transparent_black()
                                    })
                                    .when(selected, |row| row.bg(translucent(ACCENT_BLUE, 0.08)))
                                    .when(!selected && index % 2 != 0, |row| {
                                        row.bg(translucent(SIDEBAR, 0.22))
                                    })
                                    .hover(|style| {
                                        style.bg(if selected { translucent(ACCENT_BLUE, 0.11) } else { color(HOVER) })
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
                                div().w_full().h(px(if view.compact_view { ROW_HEIGHT * 0.78 } else { ROW_HEIGHT })).flex()
                                    .child(div().w(px(14.)).h_full().flex_shrink_0())
                                    .child(row.with_animation(
                                        ("selection-light", selected as usize),
                                        Animation::new(Duration::from_millis(150)).with_easing(gpui::ease_out_quint()),
                                        move |row, delta| {
                                            if selected { row.bg(translucent(ACCENT_BLUE, 0.04 + 0.04 * delta)) } else { row }
                                        },
                                    ))
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
