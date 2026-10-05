use crate::{
    app::FileManager,
    state::preview::Preview,
    ui::{
        components::{navigation_button, toolbar_button},
        theme::*,
    },
};
use gpui::{App, Context, Div, SharedString, StyledText, Window, div, img, prelude::*, px};
use std::sync::OnceLock;

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
    pub(crate) fn preview_panel(
        &self,
        expanded: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex()
            .flex_col()
            .min_h_0()
            .min_w_0()
            .when(expanded, |panel| panel.flex_1())
            .when(!expanded, |panel| {
                panel
                    .w(window.viewport_size().width * 0.48)
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
            .child(
                div()
                    .id(if expanded {
                        "expanded-preview-body"
                    } else {
                        "preview-body"
                    })
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .map(|mut body| {
                        body.style().restrict_scroll_to_axis = Some(true);
                        body
                    })
                    .child(match &self.preview {
                        Preview::Image(path)
                        | Preview::ArchiveImage(crate::infrastructure::archive::Materialized {
                            path,
                            ..
                        }) => div()
                            .p_3()
                            .child(
                                img(path.clone())
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
                        Preview::Text(text) => div()
                            .p_3()
                            .text_size(px(12.))
                            .child(text.clone())
                            .into_any_element(),
                        Preview::Code(code) => div()
                            .id(if expanded {
                                "expanded-code-preview"
                            } else {
                                "code-preview"
                            })
                            .flex()
                            .w_full()
                            .py_3()
                            .overflow_x_scroll()
                            .map(|mut code| {
                                code.style().restrict_scroll_to_axis = Some(true);
                                code
                            })
                            .font_family(code_font(cx))
                            .text_size(px(12.))
                            .line_height(px(18.))
                            .whitespace_nowrap()
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .px_3()
                                    .text_right()
                                    .text_color(color(CODE_GUTTER))
                                    .child(code.line_numbers.clone()),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .pr_3()
                                    .text_color(color(CODE_TEXT))
                                    .child(
                                        StyledText::new(code.text.clone())
                                            .with_highlights(code.highlights.iter().cloned()),
                                    ),
                            )
                            .into_any_element(),
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
                    .child(self.details_panel(false, cx)),
            )
    }
}
