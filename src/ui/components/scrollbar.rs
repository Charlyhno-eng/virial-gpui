//! A slim overlay scrollbar for vertically scrolling views.
//!
//! GPUI scrolls the file list for us with the wheel and the trackpad, but a
//! graphical list without a thumb gives no sense of size or position. This
//! paints a proportional thumb on the right edge whose geometry derives from
//! the list's own scroll handle, so it can never disagree with the contents.

use gpui::{Div, Styled, UniformListScrollHandle, div, prelude::*, px};

/// Width of the thumb, and of the slightly wider invisible hit area.
const THUMB_WIDTH: f32 = 4.;
const HIT_WIDTH: f32 = 14.;
/// Keep the thumb clickable even when the list is very long.
const MIN_THUMB: f32 = 24.;

/// Thumb geometry inside its track.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Thumb {
    /// Offset of the thumb top.
    pub(crate) top: f32,
    /// Thumb height.
    pub(crate) height: f32,
    /// Portion of the content visible, between 0 and 1.
    pub(crate) ratio: f32,
}

/// Geometry of the thumb from raw measurements.
///
/// `viewport` is the height of the visible area, `content` the full scrollable
/// height. Returns `None` when there is nothing to scroll: an unmeasured list
/// (`content <= viewport`) or one that already fits.
pub(crate) fn thumb(viewport: f32, content: f32, offset: f32) -> Option<Thumb> {
    if viewport <= 0. || content <= viewport {
        return None;
    }
    let travel = viewport * (1. - viewport / content);
    let height = viewport - travel;
    let fraction = (offset / (content - viewport)).clamp(0., 1.);
    Some(Thumb {
        top: fraction * travel,
        height: height.max(MIN_THUMB).min(viewport),
        ratio: viewport / content,
    })
}

/// Thumb for a list of `item_count` rows, or `None` while there is nothing to
/// scroll. Cheap enough to call on every render of the list.
pub(crate) fn geometry(handle: &UniformListScrollHandle, item_count: usize) -> Option<Thumb> {
    if item_count == 0 {
        return None;
    }
    let base = handle.0.borrow().base_handle.clone();
    let viewport = f32::from(base.bounds().size.height);
    let overflow = f32::from(base.max_offset().height);
    if viewport <= 0. || overflow <= 0. {
        return None;
    }
    thumb(viewport, viewport + overflow, f32::from(base.offset().y))
}

/// Paint the thumb as an overlay inside a `relative` container.
///
/// Renders nothing until [`geometry`] reports a scrollable list, so the thumb
/// never flickers on the opening frame.
pub(crate) fn render(handle: &UniformListScrollHandle, item_count: usize, tint: gpui::Rgba) -> Div {
    let Some(thumb) = geometry(handle, item_count) else {
        return div();
    };
    div()
        .absolute()
        .top(px(0.))
        .right(px(0.))
        .bottom(px(0.))
        .w(px(HIT_WIDTH))
        .flex()
        .justify_end()
        .pr(px(2.))
        .child(
            div()
                .w(px(THUMB_WIDTH))
                .h(px(thumb.height))
                .mt(px(thumb.top))
                .rounded_full()
                .bg(tint),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_to_scroll_yields_no_thumb() {
        assert!(thumb(0., 0., 0.).is_none());
        assert!(thumb(500., 0., 0.).is_none());
        // Content exactly filling the viewport has no overflow to reach.
        assert!(thumb(500., 500., 0.).is_none());
        assert!(thumb(500., 400., 0.).is_none());
    }

    #[test]
    fn a_viewport_showing_half_the_content() {
        let thumb = thumb(100., 200., 0.).expect("scrollable");
        assert_eq!(thumb.height, 50.);
        assert_eq!(thumb.top, 0.);
        assert_eq!(thumb.ratio, 0.5);
    }

    #[test]
    fn the_thumb_reaches_the_bottom_when_scrolled_to_the_end() {
        let thumb = thumb(100., 200., 100.).expect("scrollable");
        assert_eq!(thumb.top, 50.);
        assert_eq!(thumb.height, 50.);
    }

    #[test]
    fn offsets_outside_the_range_are_clamped() {
        assert_eq!(thumb(100., 200., -40.).expect("scrollable").top, 0.);
        assert_eq!(thumb(100., 200., 999.).expect("scrollable").top, 50.);
    }

    #[test]
    fn a_long_list_keeps_a_grabbable_thumb() {
        // 10 000 rows in a 600 px viewport: proportional height is subpixel.
        let thumb = thumb(600., 600_000., 0.).expect("scrollable");
        assert_eq!(thumb.height, MIN_THUMB);
        assert!(thumb.top < 600.);
    }

    #[test]
    fn an_unmeasured_list_has_no_thumb() {
        let handle = UniformListScrollHandle::new();
        assert!(geometry(&handle, 0).is_none());
        // Fresh handles report neither bounds nor scroll range yet.
        assert!(geometry(&handle, 42).is_none());
    }
}
