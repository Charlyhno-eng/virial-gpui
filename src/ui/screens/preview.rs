use crate::infrastructure::image_edit::{ExportFormat, ImageEdit};
use crate::{
    app::FileManager,
    state::preview::Preview,
    ui::{
        components::{navigation_button, toolbar_button},
        theme::*,
    },
};
use gpui::{
    App, Bounds, Context, Div, Pixels, SharedString, StyledText, Window, div, img, prelude::*, px,
};
use std::sync::OnceLock;

fn media_time(seconds: f64) -> String {
    let seconds = seconds.max(0.) as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
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
    pub(crate) fn scroll_preview_to_line(&self) {
        let scroll = &self.preview_scroll;
        if let Some(line) = scroll.bounds_for_item(self.preview_line) {
            let mut offset = scroll.offset();
            // Use the rendered viewport and line bounds, and preserve horizontal scrolling.
            offset.y =
                line_scroll_offset(scroll.bounds(), line, offset.y, scroll.max_offset().height);
            scroll.set_offset(offset);
        }
    }

    pub(crate) fn preview_width(&self, window: &Window) -> f32 {
        let width = f32::from(window.viewport_size().width);
        self.layout
            .preview_width
            .unwrap_or(width * 0.48)
            .clamp(160., (width - SIDEBAR_WIDTH - 160.).max(160.))
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
        if let Some((start, width)) = self.preview_resize {
            let maximum =
                (f32::from(window.viewport_size().width) - SIDEBAR_WIDTH - 160.).max(160.);
            self.layout.preview_width =
                Some((width + f32::from(start - event.position.x)).clamp(160., maximum));
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
                    .w(px(self.preview_width(window)))
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
                    .gap_1()
                    .flex_shrink_0()
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
                            view.close_preview(cx);
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
                    .when(
                        matches!(&self.preview, Preview::Text(_) | Preview::Code(_)),
                        |body| body.flex().flex_col(),
                    )
                    .when(matches!(&self.preview, Preview::Code(_)), |body| {
                        body.overflow_hidden()
                    })
                    .when(!matches!(&self.preview, Preview::Code(_)), |body| {
                        body.overflow_y_scroll()
                    })
                    .map(|mut body| {
                        body.style().restrict_scroll_to_axis = Some(true);
                        body
                    })
                    .child(match &self.preview {
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
                        Preview::Code(code) => {
                            let mut offset = 0;
                            let lines = code
                                .text
                                .split('\n')
                                .zip(code.line_numbers.split('\n'))
                                .enumerate()
                                .map(|(index, (line, line_number))| {
                                    let start = offset;
                                    offset += line.len() + 1;
                                    let end = start + line.len();
                                    let highlights =
                                        code.highlights.iter().filter_map(|(range, style)| {
                                            let from = range.start.max(start);
                                            let to = range.end.min(end);
                                            (from < to)
                                                .then(|| (from - start..to - start, style.clone()))
                                        });
                                    div()
                                        .flex()
                                        .flex_shrink_0()
                                        .w_full()
                                        .h(px(18.))
                                        .when(
                                            self.preview_focused && self.preview_line == index,
                                            |line| line.bg(color(SELECTED)),
                                        )
                                        .child(
                                            div()
                                                .flex_shrink_0()
                                                .w(px(44.))
                                                .pr_3()
                                                .text_right()
                                                .text_color(color(CODE_GUTTER))
                                                .child(line_number.to_owned()),
                                        )
                                        .child(
                                            div()
                                                .flex_shrink_0()
                                                .pr_3()
                                                .text_color(color(CODE_TEXT))
                                                .child(
                                                    StyledText::new(line.to_owned())
                                                        .with_highlights(highlights),
                                                ),
                                        )
                                        .on_mouse_down(
                                            gpui::MouseButton::Left,
                                            cx.listener(move |view, _, window, cx| {
                                                view.preview_focused = true;
                                                view.preview_line = index;
                                                view.focus.focus(window);
                                                cx.notify();
                                                cx.stop_propagation();
                                            }),
                                        )
                                });
                            div()
                                .id(if expanded {
                                    "expanded-code-preview"
                                } else {
                                    "code-preview"
                                })
                                .flex_1()
                                .min_h_0()
                                .py_3()
                                .overflow_y_scroll()
                                .overflow_x_scroll()
                                .track_scroll(&self.preview_scroll)
                                .map(|mut code| {
                                    code.style().restrict_scroll_to_axis = Some(true);
                                    code
                                })
                                .font_family(code_font(cx))
                                .text_size(px(12.))
                                .line_height(px(18.))
                                .whitespace_nowrap()
                                .flex()
                                .flex_col()
                                .children(lines)
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    cx.listener(|view, _, window, cx| {
                                        view.preview_focused = true;
                                        view.focus.focus(window);
                                        cx.notify();
                                        cx.stop_propagation();
                                    }),
                                )
                                .into_any_element()
                        }
                        Preview::Loading => div()
                            .p_3()
                            .text_color(color(MUTED))
                            .child(self.language.text("Loading preview…"))
                            .into_any_element(),
                        Preview::Folder(_) => div().into_any_element(),
                        Preview::Unavailable => div()
                            .p_3()
                            .text_color(color(MUTED))
                            .child(self.language.text("No content preview available"))
                            .into_any_element(),
                    })
                    .when(!matches!(&self.preview, Preview::Code(_)), |body| {
                        body.child(self.details_panel(false, cx))
                    }),
            )
            .when(!expanded, |panel| {
                panel.child(
                    div()
                        .id("preview-resize")
                        .absolute()
                        .left_0()
                        .top_0()
                        .w(px(6.))
                        .h_full()
                        .cursor(gpui::CursorStyle::ResizeLeftRight)
                        .hover(|style| style.bg(color(ACCENT)))
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(|view, event: &gpui::MouseDownEvent, window, cx| {
                                view.preview_resize =
                                    Some((event.position.x, view.preview_width(window)));
                                cx.stop_propagation();
                            }),
                        ),
                )
            })
    }
}

#[cfg(test)]
#[path = "../../../tests/ui/preview.rs"]
mod tests;
