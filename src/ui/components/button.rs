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
