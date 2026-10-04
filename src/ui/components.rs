use crate::{icons::icon, theme::*};
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
        .gap_2()
        .px_3()
        .h(px(34.))
        .rounded_md()
        .text_size(px(13.))
        .text_color(color(if active { ACCENT } else { TEXT }))
        .bg(color(if active { SELECTED } else { SURFACE }))
        .when(enabled, |button| {
            button
                .cursor_pointer()
                .hover(|style| style.bg(color(HOVER)))
        })
        .when(!enabled, |button| button.opacity(0.35))
        .child(icon(symbol, 16., if active { ACCENT } else { TEXT }))
        .child(label)
}

pub fn section_label(label: &'static str) -> Div {
    div()
        .px_3()
        .pt_5()
        .pb_2()
        .text_size(px(11.))
        .text_color(color(MUTED))
        .child(label)
}
