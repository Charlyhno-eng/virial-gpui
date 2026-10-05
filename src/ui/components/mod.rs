mod button;
pub(crate) mod input;
mod modal;
mod sidebar;
pub(super) mod titlebar;
mod toolbar;

pub(super) use button::{navigation_button, toolbar_button};

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
        .pt_3()
        .pb_1()
        .text_size(px(9.5))
        .text_color(color(MUTED))
        .child(label)
}
mod global_search;

/// Recede during the read, then settle the new contents over a total of ~150 ms.
pub fn folder_transition<E: IntoElement + Styled + 'static>(
    element: E,
    generation: usize,
    outgoing: bool,
) -> AnimationElement<E> {
    element.with_animation(
        (
            if outgoing { "folder-out" } else { "folder-in" },
            generation,
        ),
        Animation::new(Duration::from_millis(if outgoing { 65 } else { 85 }))
            .with_easing(gpui::ease_out_quint()),
        move |element, delta| {
            let depth = if outgoing { delta } else { 1. - delta };
            element
                .relative()
                .top(px(2. * depth))
                .opacity(1. - 0.12 * depth)
        },
    )
}
