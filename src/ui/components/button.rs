use crate::{ui::icons::icon, ui::theme::*};
use gpui::{Div, Stateful, div, prelude::*, px};

pub fn toolbar_button(
    id: &'static str,
    symbol: &'static str,
    label: &'static str,
    enabled: bool,
    active: bool,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap_1()
        .px_2()
        .h(px(28.))
        .rounded_md()
        .text_size(px(11.))
        .text_color(color(if active { ACCENT } else { TEXT }))
        .when(active, |button| button.bg(color(SELECTED)))
        .when(enabled, |button| {
            button
                .cursor_pointer()
                .hover(|style| style.bg(color(HOVER)))
                .active(|style| style.bg(color(SELECTED)).text_color(color(ACCENT)))
        })
        .when(!enabled, |button| button.opacity(0.35))
        .child(icon(symbol, 14., if active { ACCENT } else { MUTED }))
        .child(label)
}

struct ButtonTooltip(&'static str);

impl gpui::Render for ButtonTooltip {
    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .rounded_sm()
            .bg(color(SURFACE))
            .border_1()
            .border_color(color(BORDER))
            .text_color(color(TEXT))
            .text_size(px(11.))
            .child(self.0)
    }
}

pub fn navigation_button(
    id: &'static str,
    symbol: &'static str,
    label: &'static str,
    enabled: bool,
) -> Stateful<Div> {
    toolbar_button(id, symbol, "", enabled, false)
        .tooltip(move |_, cx| cx.new(|_| ButtonTooltip(label)).into())
}
