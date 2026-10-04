use crate::{
    app::FileManager,
    state::preview::Preview,
    ui::{
        components::{navigation_button, toolbar_button},
        theme::*,
    },
};
use gpui::{Context, Div, div, img, prelude::*, px};

impl FileManager {
    pub(crate) fn preview_panel(&self, expanded: bool, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .min_h_0()
            .min_w_0()
            .when(expanded, |panel| panel.flex_1())
            .when(!expanded, |panel| {
                panel
                    .w(px(280.))
                    .flex_shrink_0()
                    .border_l_1()
                    .border_color(color(BORDER))
            })
            .bg(translucent(SURFACE, 0.24))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .p_2()
                    .gap_1()
                    .flex_shrink_0()
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
                    .child(match &self.preview {
                        Preview::Image(path) => div()
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
                                    .h(px(if expanded { 520. } else { 220. })),
                            )
                            .into_any_element(),
                        Preview::Text(text) => div()
                            .p_3()
                            .text_size(px(12.))
                            .child(text.clone())
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
