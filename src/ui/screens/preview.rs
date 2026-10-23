use crate::infrastructure::image_edit::{ExportFormat, ImageEdit};
use crate::{
    app::FileManager,
    state::{html, markdown, preview::Preview},
    ui::{
        components::{navigation_button, toolbar_button},
        theme::*,
    },
};
use gpui::{
    App, Bounds, Context, Div, Pixels, SharedString, StyledText, Window, div, img, prelude::*, px,
    uniform_list,
};
use std::sync::OnceLock;

fn media_time(seconds: f64) -> String {
    let seconds = seconds.max(0.) as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn resized_columns(right: f32, left: Option<f32>, delta: f32, maximum: f32) -> (f32, Option<f32>) {
    let delta = delta.clamp(
        160. - right,
        left.map_or(maximum - right, |left| left - 160.),
    );
    (right + delta, left.map(|left| left - delta))
}

fn default_column_width(available: f32, parents: usize, file: bool, column: usize) -> f32 {
    let weight = if file && column == parents + 1 {
        2.
    } else {
        1.
    };
    let units = (parents + 2 + usize::from(file)).min(crate::state::preview::MAX_PREVIEW_COLUMNS);
    (available * weight / units as f32).max(160.)
}

fn media_button(id: &'static str, label: &'static str) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .px_2()
        .py_1()
        .rounded_md()
        .text_size(px(12.))
        .cursor_pointer()
        .hover(|style| style.bg(color(HOVER)))
        .child(label)
}

fn line_scroll_offset(
    viewport: Bounds<Pixels>,
    line: Bounds<Pixels>,
    offset: Pixels,
    max_offset: Pixels,
) -> Pixels {
    let offset = if line.top() + offset < viewport.top() {
        viewport.top() - line.top()
    } else if line.bottom() + offset > viewport.bottom() {
        viewport.bottom() - line.bottom()
    } else {
        offset
    };
    offset.clamp(-max_offset, px(0.))
}

fn code_font(cx: &App) -> SharedString {
    static FONT: OnceLock<SharedString> = OnceLock::new();
    FONT.get_or_init(|| {
        // GPUI's Linux backend requires a family name, not the generic
        // "monospace" alias (which falls back to the UI's proportional font).
        let available = cx.text_system().all_font_names();
        [
            "Cascadia Code",
            "JetBrains Mono",
            "Fira Code",
            "DejaVu Sans Mono",
            "Noto Sans Mono",
            "Noto Mono",
            "Liberation Mono",
            "Ubuntu Mono",
            "Ubuntu Sans Mono",
            "Hack",
            "FreeMono",
            "Nimbus Mono PS",
            "Courier New",
            "Courier",
        ]
        .into_iter()
        .find(|name| available.iter().any(|font| font == name))
        .unwrap_or(".ZedMono")
        .into()
    })
    .clone()
}

impl FileManager {
    /// Is the preview showing the syntax-colored source list rather than a
    /// rendered or plain document? Those views share one virtualized list and
    /// therefore one scroll handle, so every keyboard path must agree.
    pub(crate) fn preview_is_source_list(&self) -> bool {
        matches!(self.preview, Preview::Code(_) | Preview::Html(_, false))
    }

    pub(crate) fn scroll_preview_to_line(&self) {
        if self.preview_is_source_list() {
            let strategy = if px(self.preview_line as f32 * 18.)
                < -self.code_preview_scroll.0.borrow().base_handle.offset().y
            {
                gpui::ScrollStrategy::Top
            } else {
                gpui::ScrollStrategy::Bottom
            };
            self.code_preview_scroll
                .scroll_to_item(self.preview_line, strategy);
            return;
        }
        let scroll = &self.preview_scroll;
        if let Some(line) = scroll.bounds_for_item(self.preview_line) {
            let mut offset = scroll.offset();
            // Use the rendered viewport and line bounds, and preserve horizontal scrolling.
            offset.y =
                line_scroll_offset(scroll.bounds(), line, offset.y, scroll.max_offset().height);
            scroll.set_offset(offset);
        }
    }

    /// The shared syntax-colored source list, used by every document format
    /// that offers a source view (code, Markdown, HTML).
    fn source_view(&self, id: &'static str, cx: &mut Context<Self>) -> impl IntoElement {
        uniform_list(
            id,
            self.preview
                .source_lines()
                .map_or(0, |code| code.lines.len()),
            cx.processor(|view, range: std::ops::Range<usize>, _, cx| {
                let Some(code) = view.preview.source_lines() else {
                    return Vec::new();
                };
                range
                    .map(|index| {
                        let line = &code.lines[index];
                        div()
                            .flex()
                            .flex_shrink_0()
                            .w_full()
                            .h(px(18.))
                            .when(view.preview_focused && view.preview_line == index, |line| {
                                line.bg(color(SELECTED))
                            })
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .w(px(44.))
                                    .pr_3()
                                    .text_right()
                                    .text_color(color(CODE_GUTTER))
                                    .child((index + 1).to_string()),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .pr_3()
                                    .text_color(color(CODE_TEXT))
                                    .child(
                                        StyledText::new(line.text.clone())
                                            .with_highlights(line.styled_highlights()),
                                    ),
                            )
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(move |view, _, window, cx| {
                                    view.preview_focused = true;
                                    view.preview_active_column = view.preview_history.len() + 1;
                                    view.preview_line = index;
                                    view.focus.focus(window);
                                    cx.notify();
                                    cx.stop_propagation();
                                }),
                            )
                            .into_any_element()
                    })
                    .collect()
            }),
        )
        .with_width_from_item(self.preview.source_lines().map(|code| code.widest_line))
        .with_horizontal_sizing_behavior(gpui::ListHorizontalSizingBehavior::Unconstrained)
        .track_scroll(self.code_preview_scroll.clone())
        .flex_1()
        .min_h_0()
        .font_family(code_font(cx))
        .text_size(px(12.))
        .line_height(px(18.))
        .whitespace_nowrap()
    }

    pub(crate) fn preview_width(&self, window: &Window) -> f32 {
        let width = f32::from(window.viewport_size().width);
        self.layout
            .preview_width
            .unwrap_or_else(|| {
                default_column_width(width - self.sidebar_width, 0, self.preview_is_file(), 1)
            })
            .clamp(160., (width - self.sidebar_width - 160.).max(160.))
    }

    pub(crate) fn resize_preview(
        &mut self,
        event: &gpui::MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !event.dragging() {
            self.preview_resize = None;
            return;
        }
        if let Some((start, width, column, left)) = self.preview_resize {
            let maximum =
                (f32::from(window.viewport_size().width) - self.sidebar_width - 160.).max(160.);
            let delta = f32::from(start - event.position.x);
            let (right, left) = resized_columns(width, left, delta, maximum);
            self.preview_column_widths
                .resize(self.preview_history.len() + 2, None);
            self.preview_column_widths[column] = Some(right);
            if let Some(left) = left {
                self.preview_column_widths[column - 1] = Some(left);
            }
            if self.preview_history.is_empty() {
                self.layout.preview_width = Some(right);
            }
            cx.notify();
        }
    }

    fn media_preview_panel(
        &self,
        media: &crate::infrastructure::media::Media,
        expanded: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        use crate::infrastructure::media::Control;
        let snapshot = media.snapshot();
        let mut panel = div().flex().flex_col().p_3().gap_3();
        if snapshot.failed {
            return panel.text_color(color(MUTED)).child(self.language.text(
                "Cannot preview media. Install libmpv2 and check that the file is playable.",
            ));
        }
        if !snapshot.ready {
            return panel
                .text_color(color(MUTED))
                .child(self.language.text("Loading preview…"));
        }
        if media.video {
            panel = panel.child(
                div()
                    .w_full()
                    .h(px(if expanded { 540. } else { 270. }))
                    .bg(color(0x000000))
                    .children(self.preview_media_image.as_ref().map(|frame| {
                        img(frame.clone())
                            .w_full()
                            .h_full()
                            .object_fit(gpui::ObjectFit::Contain)
                    })),
            );
        } else {
            panel = panel.child(
                div()
                    .flex()
                    .justify_center()
                    .py_6()
                    .child(crate::ui::icons::icon("music", 64., MUTED)),
            );
        }
        let paused = snapshot.paused;
        let muted = snapshot.muted;
        panel
            .child(div().text_color(color(MUTED)).child(format!(
                "{} / {}",
                media_time(snapshot.position),
                media_time(snapshot.duration),
            )))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .child(
                        media_button(
                            "media-play",
                            self.language.text(if paused { "Play" } else { "Pause" }),
                        )
                        .on_click(cx.listener(move |view, _, _, _| {
                            view.media_control(Control::Pause(!paused))
                        })),
                    )
                    .child(media_button("media-back", "−10 s").on_click(
                        cx.listener(|view, _, _, _| view.media_control(Control::Seek(-10.))),
                    ))
                    .child(media_button("media-forward", "+10 s").on_click(
                        cx.listener(|view, _, _, _| view.media_control(Control::Seek(10.))),
                    ))
                    .child(
                        media_button("media-restart", self.language.text("Restart")).on_click(
                            cx.listener(|view, _, _, _| view.media_control(Control::Restart)),
                        ),
                    )
                    .child(
                        media_button(
                            "media-mute",
                            self.language.text(if muted { "Unmute" } else { "Mute" }),
                        )
                        .on_click(cx.listener(move |view, _, _, _| {
                            view.media_control(Control::Mute(!muted))
                        })),
                    ),
            )
    }

    fn media_control(&self, control: crate::infrastructure::media::Control) {
        if let Preview::Media(media) = &self.preview {
            media.control(control);
        }
    }

    fn folder_preview(&self, column: Option<usize>, cx: &mut Context<Self>) -> gpui::AnyElement {
        let entries = if let Some(index) = column {
            &self.preview_history[index].entries
        } else if let Preview::Folder(entries) = &self.preview {
            entries
        } else {
            unreachable!()
        };
        let scroll = column.map_or_else(
            || self.code_preview_scroll.clone(),
            |index| self.preview_history[index].scroll.clone(),
        );
        if entries.is_empty() {
            return div()
                .id((
                    "empty-folder-preview",
                    column.unwrap_or(self.preview_history.len()),
                ))
                .flex_1()
                .p_3()
                .text_color(color(MUTED))
                .child(self.language.text("Empty folder"))
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(move |view, _, window, cx| {
                        view.preview_active_column =
                            column.map_or(view.preview_history.len() + 1, |index| index + 1);
                        view.preview_focused = false;
                        view.focus.focus(window);
                        cx.notify();
                    }),
                )
                .into_any_element();
        }
        uniform_list(
            (
                "folder-preview",
                column.unwrap_or(self.preview_history.len()),
            ),
            entries.len(),
            cx.processor(move |view, range: std::ops::Range<usize>, _, cx| {
                let (entries, selected) = if let Some(index) = column {
                    let Some(parent) = view.preview_history.get(index) else {
                        return Vec::new();
                    };
                    let selected = parent
                        .selection
                        .primary()
                        .and_then(|index| parent.entries.get(index))
                        .map(|entry| &entry.path);
                    (&parent.entries, selected)
                } else if let Preview::Folder(entries) = &view.preview {
                    (
                        entries,
                        view.preview_selection
                            .primary()
                            .and_then(|index| entries.get(index))
                            .map(|entry| &entry.path),
                    )
                } else {
                    return Vec::new();
                };
                range
                    .map(|index| {
                        let entry = &entries[index];
                        let path: std::sync::Arc<std::path::Path> = entry.path.as_path().into();
                        let active = view.preview_active_column
                            == column.map_or(view.preview_history.len() + 1, |index| index + 1);
                        div()
                            .id(("folder-preview-entry", index))
                            .h(px(36.))
                            .border_l_2()
                            .border_color(gpui::transparent_black())
                            .px_3()
                            .flex()
                            .items_center()
                            .gap_2()
                            .cursor_pointer()
                            .hover(|style| style.bg(color(HOVER)))
                            .when(selected == Some(&entry.path), |row| {
                                row.bg(color(if active { SELECTED } else { HOVER }))
                                    .when(active, |row| row.border_color(color(ACCENT)))
                            })
                            .child(crate::ui::icons::file_icon(entry.icon(), 18.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_ellipsis()
                                    .child(entry.name.clone()),
                            )
                            .when(entry.browsable(), |row| {
                                row.child(crate::ui::icons::icon("forward", 12., MUTED))
                            })
                            .on_click(cx.listener(
                                move |view, event: &gpui::ClickEvent, window, cx| {
                                    if !event.standard_click() {
                                        return;
                                    }
                                    cx.stop_propagation();
                                    let entry = if let Some(column) = column {
                                        view.preview_history
                                            .get(column)
                                            .and_then(|parent| parent.entries.get(index))
                                    } else if let Preview::Folder(entries) = &view.preview {
                                        entries.get(index)
                                    } else {
                                        None
                                    }
                                    .filter(|entry| entry.path == path.as_ref())
                                    .cloned();
                                    let Some(entry) = entry else {
                                        return;
                                    };
                                    view.focus.focus(window);
                                    if event.click_count() >= 2 && !event.is_keyboard() {
                                        view.open(entry, cx);
                                    } else if event.is_keyboard() {
                                        if let Some(index) = column {
                                            view.preview_column_child(index, entry, cx);
                                        } else {
                                            view.preview_child(entry, cx);
                                        }
                                    } else {
                                        view.schedule_preview(cx, move |view, cx| {
                                            if let Some(index) = column {
                                                view.preview_column_child(index, entry, cx);
                                            } else {
                                                view.preview_child(entry, cx);
                                            }
                                        });
                                    }
                                },
                            ))
                            .into_any_element()
                    })
                    .collect()
            }),
        )
        .track_scroll(scroll)
        .on_mouse_down(
            gpui::MouseButton::Left,
            cx.listener(move |view, _, window, cx| {
                view.cancel_pending_preview();
                view.preview_active_column =
                    column.map_or(view.preview_history.len() + 1, |index| index + 1);
                view.preview_focused = false;
                view.focus.focus(window);
                cx.notify();
            }),
        )
        .flex_1()
        .min_h_0()
        .into_any_element()
    }

    pub(crate) fn preview_parent_columns(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        self.preview_history
            .iter()
            .enumerate()
            .filter(|(index, _)| index + 1 >= self.visible_preview_column_start())
            .map(|(index, column)| {
                div()
                    .relative()
                    .w(px(self.preview_width_for_column(index + 1, window)))
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .min_h_0()
                    .overflow_hidden()
                    .border_l_1()
                    .border_color(color(BORDER))
                    .child(
                        div()
                            .id(("preview-parent-header", index))
                            .h(px(40.))
                            .border_b_1()
                            .border_color(color(if self.preview_active_column == index + 1 {
                                ACCENT
                            } else {
                                BORDER
                            }))
                            .px_3()
                            .flex()
                            .items_center()
                            .flex_shrink_0()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_ellipsis()
                                    .text_color(color(MUTED))
                                    .child(column.entry.name.clone()),
                            )
                            .child(
                                navigation_button(
                                    "close-parent-column",
                                    "close",
                                    self.language.text("Close"),
                                    true,
                                )
                                .on_click(cx.listener(
                                    move |view, _, _, cx| {
                                        view.close_preview_column(index, cx);
                                        cx.stop_propagation();
                                    },
                                )),
                            ),
                    )
                    .child(self.folder_preview(Some(index), cx))
                    .children(self.column_resize_handle(index + 1, cx))
            })
            .collect()
    }

    pub(crate) fn preview_column_width(&self, window: &Window) -> f32 {
        self.preview_width_for_column(self.preview_history.len() + 1, window)
    }

    fn preview_width_for_column(&self, column: usize, window: &Window) -> f32 {
        if let Some(width) = self.preview_column_widths.get(column).copied().flatten() {
            return width;
        }
        if self.preview_history.is_empty() {
            return self.preview_width(window);
        }
        let width = f32::from(window.viewport_size().width) - self.sidebar_width;
        default_column_width(
            width,
            self.preview_history.len(),
            self.preview_is_file(),
            column,
        )
    }

    fn column_resize_handle(
        &self,
        column: usize,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Stateful<Div>> {
        if column <= self.visible_preview_column_start() {
            return None;
        }
        Some(
            div()
                .id(("preview-resize", column))
                .absolute()
                .left_0()
                .top_0()
                .bottom_0()
                .w(px(6.))
                .cursor(gpui::CursorStyle::ResizeLeftRight)
                .hover(|style| style.bg(color(ACCENT)))
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(move |view, event: &gpui::MouseDownEvent, window, cx| {
                        let left =
                            (column > 1).then(|| view.preview_width_for_column(column - 1, window));
                        view.preview_resize = Some((
                            event.position.x,
                            view.preview_width_for_column(column, window),
                            column,
                            left,
                        ));
                        cx.stop_propagation();
                    }),
                ),
        )
    }

    pub(crate) fn preview_panel(
        &self,
        expanded: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .relative()
            .flex()
            .flex_col()
            .min_h_0()
            .min_w_0()
            .when(expanded, |panel| panel.flex_1())
            .when(!expanded, |panel| {
                panel
                    .w(px(self.preview_column_width(window)))
                    .flex_shrink_0()
                    .border_l_1()
                    .border_color(color(BORDER))
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .p_2()
                    .border_b_1()
                    .border_color(color(
                        if self.preview_active_column == self.preview_history.len() + 1 {
                            ACCENT
                        } else {
                            BORDER
                        },
                    ))
                    .gap_1()
                    .flex_shrink_0()
                    .when(!self.preview_history.is_empty(), |toolbar| {
                        toolbar.child(
                            navigation_button(
                                "preview-back",
                                "back",
                                self.language.text("Back"),
                                true,
                            )
                            .on_click(cx.listener(|view, _, _, cx| view.preview_back(cx))),
                        )
                    })
                    .when(
                        matches!(self.preview, Preview::Folder(_))
                            || !self.preview_history.is_empty(),
                        |toolbar| {
                            toolbar.child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .px_2()
                                    .text_ellipsis()
                                    .text_color(color(MUTED))
                                    .child(
                                        self.preview_path
                                            .as_ref()
                                            .map(|path| path.display().to_string())
                                            .unwrap_or_default(),
                                    ),
                            )
                        },
                    )
                    .when(
                        matches!(
                            &self.preview,
                            Preview::Image(..) | Preview::ArchiveImage(..)
                        ),
                        |toolbar| {
                            let enabled = !self.busy
                                && self.location != crate::domain::location::Location::Trash;
                            toolbar
                                .child(
                                    navigation_button(
                                        "convert-image",
                                        "image-convert",
                                        self.language.text("Convert image…"),
                                        enabled,
                                    )
                                    .on_click(cx.listener(
                                        |view, _, window, cx| {
                                            view.image_export_dialog(
                                                ImageEdit::Convert(ExportFormat::Png),
                                                window,
                                                cx,
                                            );
                                        },
                                    )),
                                )
                                .child(
                                    navigation_button(
                                        "remove-image-background",
                                        "background-remove",
                                        self.language.text("Remove background…"),
                                        enabled,
                                    )
                                    .on_click(cx.listener(
                                        |view, _, window, cx| {
                                            view.image_export_dialog(
                                                ImageEdit::RemoveBackground,
                                                window,
                                                cx,
                                            );
                                        },
                                    )),
                                )
                        },
                    )
                    .when(
                        expanded && matches!(&self.preview, Preview::Code(_)),
                        |toolbar| {
                            toolbar.child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .px_2()
                                    .text_color(color(MUTED))
                                    .text_ellipsis()
                                    .child(
                                        self.preview_path
                                            .as_ref()
                                            .map(|path| path.display().to_string())
                                            .unwrap_or_default(),
                                    ),
                            )
                        },
                    )
                    .when(matches!(&self.preview, Preview::Html(_, _)), |toolbar| {
                        toolbar.child(
                            navigation_button(
                                "toggle-html",
                                "eye",
                                if matches!(&self.preview, Preview::Html(_, true)) {
                                    self.language.text("Source view")
                                } else {
                                    self.language.text("Page preview")
                                },
                                true,
                            )
                            .on_click(cx.listener(
                                |view, _, window, cx| {
                                    if let Preview::Html(_, rendered) = &mut view.preview {
                                        *rendered = !*rendered;
                                    }
                                    view.focus.focus(window);
                                    cx.notify();
                                },
                            )),
                        )
                    })
                    .child(
                        navigation_button(
                            "expand-preview",
                            "fullscreen",
                            self.language.text("Full screen · Space"),
                            true,
                        )
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.preview_expanded = !view.preview_expanded;
                            view.focus.focus(window);
                            cx.notify();
                        })),
                    )
                    .child(
                        toolbar_button(
                            "close-preview",
                            "close",
                            self.language.text("Close"),
                            true,
                            false,
                        )
                        .on_click(cx.listener(|view, _, window, cx| {
                            if view.preview_history.is_empty() {
                                view.close_preview(cx);
                            } else {
                                view.preview_back(cx);
                            }
                            view.focus.focus(window);
                        })),
                    ),
            )
            .when(
                matches!(
                    &self.preview,
                    Preview::Image(..) | Preview::ArchiveImage(..)
                ),
                |panel| {
                    panel
                        .when(self.busy, |panel| {
                            panel.child(
                                div()
                                    .px_3()
                                    .py_2()
                                    .text_color(color(MUTED))
                                    .child(self.language.text("Working…")),
                            )
                        })
                        .children(self.error.as_ref().map(|error| {
                            div()
                                .px_3()
                                .py_2()
                                .text_color(color(ERROR))
                                .child(error.clone())
                        }))
                },
            )
            .child(
                div()
                    .id(if expanded {
                        "expanded-preview-body"
                    } else {
                        "preview-body"
                    })
                    .flex_1()
                    .min_h_0()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|view, _, window, cx| {
                            if view.preview_is_file() && !matches!(view.preview, Preview::Editing) {
                                view.preview_active_column = view.preview_history.len() + 1;
                                view.preview_focused = true;
                                view.focus.focus(window);
                                cx.notify();
                            }
                        }),
                    )
                    .when(
                        matches!(
                            &self.preview,
                            Preview::Text(_)
                                | Preview::Code(_)
                                | Preview::Editing
                                | Preview::Folder(_)
                                | Preview::Markdown(_)
                                | Preview::Html(_, _)
                        ),
                        |body| body.flex().flex_col(),
                    )
                    .when(
                        matches!(
                            &self.preview,
                            Preview::Code(_)
                                | Preview::Editing
                                | Preview::Folder(_)
                                | Preview::Markdown(_)
                                | Preview::Html(_, _)
                        ),
                        |body| body.overflow_hidden(),
                    )
                    // The source views bring their own scroll container, so
                    // the body must not add a second one on top of it.
                    .when(
                        !self.preview_is_source_list()
                            && !matches!(
                                &self.preview,
                                Preview::Editing
                                    | Preview::Markdown(_)
                                    | Preview::Html(_, _)
                                    | Preview::Folder(_)
                            ),
                        |body| body.overflow_y_scroll(),
                    )
                    .map(|mut body| {
                        body.style().restrict_scroll_to_axis = Some(true);
                        body
                    })
                    .child(match &self.preview {
                        // The editor replaces the read-only preview entirely.
                        Preview::Editing => match self.editor.clone() {
                            Some(editor) => editor.into_any_element(),
                            None => div().into_any_element(),
                        },
                        Preview::Image(path, _)
                        | Preview::Pdf(crate::infrastructure::archive::Materialized {
                            path, ..
                        })
                        | Preview::ArchiveImage(
                            crate::infrastructure::archive::Materialized { path, .. },
                            _,
                        ) => div()
                            .p_3()
                            .child(
                                img(path.clone())
                                    .image_cache(&self.preview_image_cache)
                                    .with_fallback({
                                        let message =
                                            self.language.text("No content preview available");
                                        move || {
                                            div()
                                                .p_3()
                                                .text_color(color(MUTED))
                                                .child(message)
                                                .into_any_element()
                                        }
                                    })
                                    .w_full()
                                    .h(px(if expanded { 640. } else { 360. })),
                            )
                            .into_any_element(),
                        Preview::Media(media) => self
                            .media_preview_panel(media, expanded, cx)
                            .into_any_element(),
                        Preview::Markdown(document) => {
                            let lines =
                                document.rendered.iter().enumerate().map(|(index, line)| {
                                    let size = match line.kind {
                                        markdown::LineKind::Heading(1) => 26.,
                                        markdown::LineKind::Heading(2) => 22.,
                                        markdown::LineKind::Heading(3) => 18.,
                                        // The mermaid caption fixes its own size.
                                        markdown::LineKind::Mermaid => 11.,
                                        _ => 13.,
                                    };
                                    if !line.cells.is_empty() {
                                        return div()
                                            .id(index)
                                            .w_full()
                                            .flex()
                                            .border_l_1()
                                            .border_color(color(BORDER))
                                            .when(
                                                line.kind == markdown::LineKind::TableHeader,
                                                |row| {
                                                    row.bg(color(HOVER))
                                                        .font_weight(gpui::FontWeight::BOLD)
                                                },
                                            )
                                            .children(line.cells.iter().map(|cell| {
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .px_2()
                                                    .py_1()
                                                    .border_r_1()
                                                    .border_b_1()
                                                    .border_color(color(BORDER))
                                                    .text_size(px(13.))
                                                    .child(
                                                        StyledText::new(cell.text.clone())
                                                            .with_highlights(
                                                                cell.highlights.iter().cloned(),
                                                            ),
                                                    )
                                            }))
                                            .into_any_element();
                                    }
                                    div()
                                        .id(index)
                                        .w_full()
                                        .pl(px(line.indent as f32 * 6.))
                                        .pr_2()
                                        .line_height(px(size * 1.55))
                                        .when(line.kind == markdown::LineKind::Quote, |row| {
                                            row.border_l_2()
                                                .border_color(color(ACCENT))
                                                .text_color(color(MUTED))
                                        })
                                        .py(px(match line.kind {
                                            markdown::LineKind::Heading(_) => 4.,
                                            markdown::LineKind::Rule => 6.,
                                            markdown::LineKind::Mermaid => 0.,
                                            _ => 1.,
                                        }))
                                        .when(line.kind == markdown::LineKind::Rule, |row| {
                                            row.h(px(1.)).bg(color(BORDER)).my_2()
                                        })
                                        .when(line.kind == markdown::LineKind::CodeBlock, |row| {
                                            row.bg(color(SURFACE))
                                        })
                                        .when(line.kind == markdown::LineKind::Mermaid, |row| {
                                            row.pt(px(6.)).text_color(color(ACCENT))
                                        })
                                        .text_size(px(size))
                                        .child(
                                            StyledText::new(line.text.clone())
                                                .with_highlights(line.highlights.iter().cloned()),
                                        )
                                        .into_any_element()
                                });
                            div()
                                .id("markdown-rendered")
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scroll()
                                .track_scroll(&self.preview_scroll)
                                .p_4()
                                .children(lines)
                                .into_any_element()
                        }
                        Preview::Html(document, show_rendered) => {
                            if *show_rendered {
                                let lines =
                                    document.rendered.iter().enumerate().map(|(index, line)| {
                                        let size = match line.kind {
                                            html::LineKind::Heading(1) => 20.,
                                            html::LineKind::Heading(2) => 17.,
                                            html::LineKind::Heading(3) => 15.,
                                            html::LineKind::Title => 12.,
                                            _ => 13.,
                                        };
                                        div()
                                            .id(index)
                                            .w_full()
                                            .pl(px(12. + line.indent as f32 * 6.))
                                            .py(px(match line.kind {
                                                html::LineKind::Heading(_) => 4.,
                                                html::LineKind::Title => 6.,
                                                html::LineKind::Rule => 6.,
                                                _ => 1.,
                                            }))
                                            .when(line.kind == html::LineKind::Rule, |row| {
                                                row.h(px(1.)).bg(color(BORDER)).my_2()
                                            })
                                            .when(line.kind == html::LineKind::CodeBlock, |row| {
                                                row.bg(color(SURFACE))
                                            })
                                            .when(line.kind == html::LineKind::Title, |row| {
                                                row.text_color(color(MUTED))
                                                    .text_size(px(12.))
                                                    .italic()
                                            })
                                            // The bullet is part of the text: gpui 0.2 has no list marker, so the
                                            // parser prefixes each item itself.
                                            .when(line.kind == html::LineKind::ListItem, |row| {
                                                row.pl(px(24. + line.indent as f32 * 6.))
                                            })
                                            .text_size(px(size))
                                            .child(
                                                StyledText::new(line.text.clone()).with_highlights(
                                                    line.highlights.iter().cloned(),
                                                ),
                                            )
                                            .into_any_element()
                                    });
                                div()
                                    .id("html-rendered")
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_y_scroll()
                                    .track_scroll(&self.preview_scroll)
                                    .py_3()
                                    .children(lines)
                                    .into_any_element()
                            } else {
                                self.source_view("html-source", cx).into_any_element()
                            }
                        }
                        Preview::Text(text) => {
                            let lines = text.split('\n').enumerate().map(|(index, line)| {
                                div()
                                    .w_full()
                                    .px_3()
                                    .text_size(px(12.))
                                    .line_height(px(18.))
                                    .when(
                                        self.preview_focused && self.preview_line == index,
                                        |line| line.bg(color(SELECTED)),
                                    )
                                    .child(line.to_owned())
                                    .on_mouse_down(
                                        gpui::MouseButton::Left,
                                        cx.listener(move |view, _, window, cx| {
                                            view.preview_focused = true;
                                            view.preview_active_column =
                                                view.preview_history.len() + 1;
                                            view.preview_line = index;
                                            view.focus.focus(window);
                                            cx.notify();
                                            cx.stop_propagation();
                                        }),
                                    )
                            });
                            div()
                                .id("text-preview-lines")
                                .flex_1()
                                .min_h_0()
                                .py_3()
                                .overflow_y_scroll()
                                .track_scroll(&self.preview_scroll)
                                .children(lines)
                                .into_any_element()
                        }
                        Preview::Code(_) => self
                            .source_view(
                                if expanded {
                                    "expanded-code-preview"
                                } else {
                                    "code-preview"
                                },
                                cx,
                            )
                            .into_any_element(),
                        Preview::Loading => div()
                            .p_3()
                            .text_color(color(MUTED))
                            .child(self.language.text("Loading preview…"))
                            .into_any_element(),
                        Preview::Folder(_) => self.folder_preview(None, cx),
                        Preview::Unavailable => div()
                            .p_3()
                            .text_color(color(MUTED))
                            .child(self.language.text("No content preview available"))
                            .into_any_element(),
                    }),
            )
            .when(!expanded, |panel| {
                panel.children(self.column_resize_handle(self.preview_history.len() + 1, cx))
            })
    }
}

#[cfg(test)]
#[path = "../../../tests/ui/preview.rs"]
mod tests;
