use crate::{
    app::FileManager,
    ui::icons::{file_icon, icon},
    ui::theme::*,
};
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
        let package = selected
            .as_ref()
            .and_then(|entry| crate::infrastructure::packages::inspect(&entry.path));
        let modified = self.preview_modified.clone().unwrap_or_else(|| "—".into());
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
            .child(if has_selection {
                file_icon(symbol, 34.)
            } else {
                icon(symbol, 34., ACCENT_BLUE).into_any_element()
            })
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
            .when_some(package.clone(), |panel, package| {
                // The package-host block, styled like the image metadata one.
                let role = self.language.text(match package.role {
                    crate::infrastructure::packages::Role::Manifest => "Package manifest",
                    crate::infrastructure::packages::Role::Lock => "Package lock file",
                    crate::infrastructure::packages::Role::Credentials => "Package credentials",
                });
                panel
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .text_size(px(11.))
                            .child(self.language.text("Registry"))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(file_icon(package.icon, 14.))
                                    .child(div().text_color(color(MUTED)).child(format!(
                                        "{} · {}",
                                        self.language.text(package.host),
                                        package.registry
                                    ))),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .text_size(px(11.))
                            .child(self.language.text("Role"))
                            .child(div().text_color(color(MUTED)).child(role)),
                    )
                    .children(package.version.as_ref().map(|version| {
                        div()
                            .flex()
                            .justify_between()
                            .text_size(px(11.))
                            .child(self.language.text("Version"))
                            .child(div().text_color(color(MUTED)).child(version.clone()))
                    }))
                    .when(package.holds_credentials, |panel| {
                        panel.child(
                            div()
                                .flex()
                                .gap_1()
                                .items_center()
                                .text_size(px(11.))
                                .text_color(color(ERROR))
                                .child(icon("info", 12., ERROR))
                                .child(
                                    self.language
                                        .text("This file carries an access key for the registry"),
                                ),
                        )
                    })
            })
            .when_some(
                (!current_folder)
                    .then(|| self.preview.image_metadata())
                    .flatten(),
                |panel, metadata| {
                    panel.children(
                        [
                            ("Format", metadata.format.clone()),
                            (
                                "Dimensions",
                                format!("{} × {} px", metadata.width, metadata.height),
                            ),
                        ]
                        .into_iter()
                        .map(|(label, value)| {
                            div()
                                .flex()
                                .justify_between()
                                .gap_3()
                                .text_size(px(11.))
                                .child(self.language.text(label))
                                .child(div().text_color(color(MUTED)).child(value))
                        }),
                    )
                },
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
        let visible_count = self.entries.len();
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
                    .child(
                        div()
                            .w(px(76.))
                            .text_right()
                            .child(self.language.text("SIZE")),
                    )
                    .child(div().w(px(150.)).flex_shrink_0().child(self.language.text("MODIFIED"))),
            )
            .child(if visible_count == 0 {
                let (heading, description) = if self.loading {
                    ("Loading folder…", "Reading directory contents")
                } else if self.error.is_some() {
                    (
                        "Folder unavailable",
                        "Choose another location or try refreshing",
                    )
                } else if self.location == crate::domain::location::Location::Trash {
                    ("Trash is empty", "Deleted items appear here")
                } else if self.location == crate::domain::location::Location::Recent {
                    (
                        "No recent files",
                        "Files opened with Virial and desktop applications appear here",
                    )
                } else {
                    (
                        "This folder is empty",
                        "Create a file or folder using the context menu",
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
                let scrollbar = crate::ui::components::scrollbar::render(
                    &self.scroll,
                    visible_count,
                    gpui::rgba(0xff8e8aa8),
                );
                div().relative().flex().flex_col().flex_1().min_h_0().overflow_hidden()
                .child(scrollbar)
                .child(uniform_list(
                    self.location.id(),
                    visible_count,
                    cx.processor(|view, range: std::ops::Range<usize>, _, cx| {
                        let paths = view.selected_paths().into();
                        range
                            .map(|index| {
                                let entry = view.entries[index].clone();
                                let recent = view.location
                                    == crate::domain::location::Location::Recent;
                                let row_height = if recent {
                                    crate::ui::theme::RECENT_ROW_HEIGHT
                                } else {
                                    ROW_HEIGHT
                                };
                                let selected = view.selection.indices.contains(&index);
                                let payload = view.drag_payload(index, &paths);
                                let trash = view.location == crate::domain::location::Location::Trash;
                                let directory = (entry.browsable() && !trash).then(|| entry.path.clone());
                                let rename_input = view.rename.as_ref()
                                    .filter(|rename| rename.source == entry.path)
                                    .map(|rename| rename.input.clone());
                                let renaming = rename_input.is_some();
                                let name = if let Some(input) = rename_input {
                                    div().flex_1().min_w_0()
                                        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                        .on_mouse_down(gpui::MouseButton::Right, |_, _, cx| cx.stop_propagation())
                                        .on_mouse_down_out(cx.listener(|view, _, window, cx| {
                                            if view.rename.is_some() {
                                                view.cancel_rename(window, cx);
                                            }
                                        }))
                                        .child(input)
                                } else if recent {
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .flex()
                                        .flex_col()
                                        .justify_center()
                                        .gap_1()
                                        .child(
                                            div()
                                                .min_w_0()
                                                .text_ellipsis()
                                                .text_size(px(12.))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .child(entry.name.clone()),
                                        )
                                        .child(
                                            div()
                                                .min_w_0()
                                                .text_size(px(10.))
                                                .text_color(color(MUTED))
                                                .text_ellipsis()
                                                .child(entry.path.display().to_string()),
                                        )
                                } else {
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_ellipsis()
                                        .text_size(px(11.))
                                        .child(entry.name.clone())
                                };
                                let row = div()
                                    .id(std::sync::Arc::<std::path::Path>::from(entry.path.clone()))
                                    .flex_1()
                                    .min_w_0()
                                    .h(px(row_height))
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
                                    .child(file_icon(entry.icon(), 19.))
                                    .child(name)
                                    .child(
                                        div()
                                            .w(px(76.))
                                            .flex_shrink_0()
                                            .text_right()
                                            .text_size(px(11.))
                                            .text_color(color(MUTED))
                                            .child(view.language.size(entry.bytes)),
                                    )
                                    .child(div().w(px(150.)).flex_shrink_0().text_size(px(10.)).text_color(color(MUTED))
                                        .child(view.entry_modified.get(&entry.path).cloned().unwrap_or_else(|| "—".into())))
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
                                            cx.stop_propagation();
                                            if view.busy || view.loading || renaming { return; }
                                            view.focus.focus(window);
                                            view.cancel_pending_preview();
                                            if !event.modifiers.control && !event.modifiers.shift {
                                                view.details_open = false;
                                                view.preview_expanded = false;
                                            }
                                            view.selection.pointer_down(index, event.modifiers.control, event.modifiers.shift);
                                            cx.notify();
                                        },
                                    ))
                                    .when(!renaming && !view.busy && !view.loading && view.marquee.is_none(), |row| {
                                        let manager = cx.entity();
                                        row.when(!trash, |row| row.on_drag(payload, move |drag, _, window, cx| {
                                            if let Some(uris) = crate::platform::file_drag::uri_list(&drag.paths) {
                                                window.start_file_drag(uris);
                                            }
                                            manager.update(cx, |view, cx| {
                                                view.cancel_pending_preview();
                                                view.details_open = false;
                                                view.preview_expanded = false;
                                                view.menu = None;
                                                cx.notify();
                                            });
                                            cx.new(|_| drag.clone())
                                        }))
                                        .on_click(cx.listener(
                                            move |view, event: &gpui::ClickEvent, _, cx| {
                                                if !event.standard_click() || event.is_keyboard() { return; }
                                                let click_count = event.click_count();
                                                if click_count != 1 {
                                                    view.cancel_pending_preview();
                                                }
                                                cx.stop_propagation();
                                                // Use the modifiers from button-down, even when
                                                // Ctrl is released before the mouse button.
                                                let modifiers = match event {
                                                    gpui::ClickEvent::Mouse(event) => event.down.modifiers,
                                                    gpui::ClickEvent::Keyboard(_) => gpui::Modifiers::default(),
                                                };
                                                view.selection.pointer_click(index, modifiers.control, modifiers.shift);
                                                if !modifiers.control && !modifiers.shift {
                                                    view.details_open = false;
                                                    view.preview_expanded = false;
                                                    if click_count >= 2 {
                                                        view.open(entry.clone(), cx);
                                                    }
                                                }
                                                if click_count == 1 && !modifiers.control && !modifiers.shift && view.selection.indices.len() == 1 {
                                                    view.defer_preview(cx);
                                                }
                                                cx.notify();
                                            },
                                        ))
                                    });
                                let row = if let Some(directory) = directory {
                                    view.drop_target(row, directory, cx)
                                } else { row };
                                div().w_full().h(px(row_height)).flex()
                                    .child(div().w(px(14.)).h_full().flex_shrink_0())
                                    // Selection changes on mouse-down must preserve the
                                    // row's pending drag gesture across the next render.
                                    .child(row.with_animation(
                                        "selection-light",
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
