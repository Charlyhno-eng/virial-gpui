use crate::{icons::icon, theme::*};
use gpui::{
    Animation, AnimationElement, AnimationExt, Div, ElementId, Stateful, div, prelude::*, px,
};
use std::time::Duration;

/// Short, one-shot fades keep navigation calm and stop requesting frames at rest.
pub fn reveal<E: IntoElement + Styled + 'static>(
    element: E,
    id: impl Into<ElementId>,
) -> AnimationElement<E> {
    element.with_animation(
        id,
        Animation::new(Duration::from_millis(150)).with_easing(gpui::ease_out_quint()),
        |element, delta| element.opacity(0.85 + 0.15 * delta),
    )
}

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

pub fn section_label(label: &'static str) -> Div {
    div()
        .px_3()
        .pt_4()
        .pb_2()
        .text_size(px(10.))
        .text_color(color(MUTED))
        .child(label)
}
