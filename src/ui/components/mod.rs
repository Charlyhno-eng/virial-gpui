mod button;
pub(crate) mod input;
mod modal;
mod sidebar;
pub(super) mod titlebar;
mod toolbar;

pub(super) use button::toolbar_button;

use crate::ui::theme::*;
use gpui::{Animation, AnimationElement, AnimationExt, Div, ElementId, div, prelude::*, px};
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

pub fn section_label(label: &'static str) -> Div {
    div()
        .px_3()
        .pt_4()
        .pb_2()
        .text_size(px(10.))
        .text_color(color(MUTED))
        .child(label)
}
