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
    // `.max()`/`.min()` are IEEE maxNum/minNum, which answer with the other
    // operand for a NaN: the height would come out repaired while `top` and
    // `ratio` stayed NaN and reached the renderer as `px(NaN)`. Reject the
    // non-finite measurements outright instead, and rely on the ordering below
    // rejecting NaN by comparison — every `<=` is false for NaN.
    if !(viewport.is_finite() && content.is_finite() && offset.is_finite()) {
        return None;
    }
    if viewport <= 0. || content <= viewport {
        return None;
    }
    // Clamp the height *before* deriving the travel: a thumb at least MIN_THUMB
    // tall shrinks the usable track, and positioning it against the unclamped
    // height would push it past the end of the track.
    let height = (viewport * viewport / content).max(MIN_THUMB).min(viewport);
    let travel = viewport - height;
    let fraction = (offset / (content - viewport)).clamp(0., 1.);
    Some(Thumb {
        top: fraction * travel,
        height,
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

    // --- non-finite and out-of-range measurements -------------------------------
    //
    // Two things the gate in `thumb` has to get right, both found by these tests:
    //
    // NaN compares false against every `<=`, so a plain `viewport <= 0. ||` gate
    // lets it through, and `.max()`/`.min()` are IEEE maxNum/minNum, which answer
    // with the other operand. The two disagree: the height came out repaired while
    // `top` and `ratio` stayed NaN and reached the renderer as `px(NaN)`.
    //
    // And `travel` must be derived from the *clamped* height. Clamping afterwards
    // let a long list push the thumb up to MIN_THUMB - 1 px past the end of the
    // track, growing without bound as the content grew.

    #[test]
    fn non_finite_or_negative_viewports_yield_no_thumb() {
        // Rejected by the explicit finiteness gate, not by accident.
        assert!(thumb(f32::INFINITY, 1_000., 0.).is_none());
        assert!(thumb(f32::INFINITY, f32::INFINITY, 0.).is_none());
        assert!(thumb(f32::NEG_INFINITY, 100., 0.).is_none());
        assert!(thumb(-10., 100., 0.).is_none());
        assert!(thumb(-1., -2., -3.).is_none());
        assert!(thumb(100., -50., 0.).is_none());
        assert!(thumb(100., f32::NEG_INFINITY, 0.).is_none());
    }

    #[test]
    fn a_nan_measurement_yields_no_thumb() {
        // Every comparison is false for NaN, so without an explicit finiteness
        // gate these sail through and `top`/`ratio` reach the renderer as NaN.
        assert!(thumb(f32::NAN, 100., 0.).is_none());
        assert!(thumb(100., f32::NAN, 0.).is_none());
        assert!(thumb(f32::NAN, f32::NAN, 0.).is_none());
        assert!(thumb(f32::NAN, f32::NAN, f32::NAN).is_none());
    }

    #[test]
    fn a_nan_offset_yields_no_thumb() {
        // `clamp` propagates NaN, so a healthy geometry with a NaN offset would
        // put NaN in `top` while height and ratio stayed correct.
        assert!(thumb(100., 200., f32::NAN).is_none());
    }

    #[test]
    fn no_measurement_ever_reaches_the_renderer_as_nan() {
        // The invariant that matters: whatever comes out is finite.
        for viewport in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0., -1., 100.] {
            for content in [
                f32::NAN,
                f32::INFINITY,
                f32::NEG_INFINITY,
                0.,
                -1.,
                100.,
                200.,
                1e9,
            ] {
                for offset in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0., 50., 1e9] {
                    if let Some(thumb) = thumb(viewport, content, offset) {
                        assert!(
                            thumb.top.is_finite(),
                            "top NaN for {viewport} {content} {offset}"
                        );
                        assert!(thumb.height.is_finite(), "height NaN");
                        assert!(thumb.ratio.is_finite(), "ratio NaN");
                        assert!(thumb.top >= 0. && thumb.top <= viewport);
                    }
                }
            }
        }
    }

    #[test]
    fn infinite_offsets_are_rejected_rather_than_clamped() {
        // An infinite offset is not a position, and letting it through the clamp
        // would put it in `top`. The finiteness gate rejects it first.
        assert!(thumb(100., 200., f32::INFINITY).is_none());
        assert!(thumb(100., 200., f32::NEG_INFINITY).is_none());
        // Large but finite offsets still clamp to the ends of the track.
        assert_eq!(thumb(100., 200., 1e9).expect("scrollable").top, 50.);
        assert_eq!(thumb(100., 200., -1e9).expect("scrollable").top, 0.);
    }

    #[test]
    fn an_infinite_content_height_is_rejected() {
        assert!(thumb(100., f32::INFINITY, 0.).is_none());
        assert!(thumb(100., f32::INFINITY, f32::INFINITY).is_none());
    }

    // --- the MIN_THUMB clamp and the track it overflows --------------------------

    #[test]
    fn content_exactly_filling_the_viewport_is_the_boundary() {
        assert!(thumb(500., 500., 0.).is_none());
        assert!(thumb(500., 500., 250.).is_none());
        // One f32 ulp below the viewport: still nothing to scroll.
        assert!(thumb(500., 499.999_94, 0.).is_none());
        // One ulp above: scrollable by a rounding error, with a thumb that covers
        // essentially the whole track.
        let just_over = thumb(500., 500.000_06, 0.).expect("scrollable by 1 ulp");
        assert_eq!(just_over.top, 0.);
        assert!((just_over.height - 500.).abs() < 1e-3);
        assert!(just_over.ratio < 1.);
    }

    #[test]
    fn the_thumb_ends_flush_with_the_track_at_the_minimum() {
        // Proportional height is exactly MIN_THUMB at `content == viewport^2 / 24`,
        // and these powers of two make that division exact in f32. The thumb then
        // still lands flush at the bottom of the track: top + height == viewport.
        for (viewport, content, expected_top) in
            [(48., 96., 24.), (96., 384., 72.), (192., 1536., 168.)]
        {
            let at_end = thumb(viewport, content, content - viewport).expect("scrollable");
            assert_eq!(at_end.height, MIN_THUMB, "viewport {viewport}");
            assert_eq!(at_end.top, expected_top, "viewport {viewport}");
            assert_eq!(
                at_end.top + at_end.height,
                viewport,
                "thumb must end flush at viewport {viewport}"
            );
        }
    }

    #[test]
    fn the_minimum_thumb_never_escapes_the_track() {
        // Regression: `height` was clamped up to MIN_THUMB *after* `travel` had
        // been derived from the unclamped height, so once the proportional
        // height fell below 24 px the thumb hung off the end of the track —
        // 23 px past it at viewport 512, content 262_144.
        for viewport in [10., 48., 512., 1_024.] {
            for content in [96., 12_288., 262_144., 1e9] {
                let Some(last) = thumb(viewport, content, content - viewport) else {
                    continue;
                };
                let proportional = viewport * viewport / content;
                assert_eq!(
                    last.height,
                    proportional.max(MIN_THUMB).min(viewport),
                    "vp {viewport} c {content}"
                );
                assert_eq!(
                    last.top + last.height,
                    viewport,
                    "thumb must end flush with the track at vp {viewport} c {content}"
                );
                assert!(last.top >= 0. && last.height > 0.);
            }
        }
    }

    #[test]
    fn the_overhang_is_gone_at_every_content_height() {
        // The old defect grew with the content and saturated at MIN_THUMB.
        // Every legitimately scrollable size must now fit its track.
        let viewport = 512.;
        for content in [12_288., 24_576., 49_152., 98_304., 196_608., 262_144., 1e9] {
            for offset in [0., content / 2., content - viewport] {
                let thumb = thumb(viewport, content, offset).expect("scrollable");
                assert!(
                    thumb.top + thumb.height <= viewport + f32::EPSILON * viewport,
                    "overflow at content {content} offset {offset}: {} > {viewport}",
                    thumb.top + thumb.height
                );
            }
        }
    }

    #[test]
    fn a_viewport_shorter_than_the_minimum_fills_the_track() {
        // Below MIN_THUMB the height is capped at the viewport; it must still not
        // overflow, which the clamp-before-travel ordering guarantees.
        for (viewport, content) in [(10., 100.), (8., 8_000.), (1., 1_000.)] {
            let thumb = thumb(viewport, content, content - viewport).expect("scrollable");
            assert_eq!(
                thumb.height, viewport,
                "height must be capped at the viewport"
            );
            assert_eq!(thumb.top, 0., "a full-height thumb cannot move");
            assert_eq!(thumb.top + thumb.height, viewport);
            assert!(thumb.ratio > 0. && thumb.ratio < 1.);
        }
    }

    #[test]
    fn just_past_the_minimum_the_thumb_still_fits() {
        // 48 px viewport: `content == viewport^2 / 24 == 96` is the last exact case.
        // A pixel more used to hang the thumb off the track.
        let thumb = thumb(48., 97., 49.).expect("scrollable");
        assert_eq!(thumb.height, MIN_THUMB);
        assert!(
            thumb.top + thumb.height <= 48. + f32::EPSILON * 48.,
            "must fit: {} > 48",
            thumb.top + thumb.height
        );
    }

    // --- invariants that must hold across the whole scrollable range -------------

    #[test]
    fn the_ratio_is_the_visible_fraction() {
        for (viewport, content) in [
            (100., 200.),
            (300., 1_000.),
            (512., 262_144.),
            (600., 600_000.),
        ] {
            let thumb = thumb(viewport, content, 0.).expect("scrollable");
            assert_eq!(thumb.ratio, viewport / content, "viewport {viewport}");
        }
    }

    #[test]
    fn the_position_advances_monotonically_with_the_offset() {
        let viewport = 600.;
        let content = 600_000.;
        let mut previous = -1.;
        for step in 0..=20 {
            let offset = (content - viewport) * step as f32 / 20.;
            let thumb = thumb(viewport, content, offset).expect("scrollable");
            assert!(thumb.top >= previous, "top went backwards at step {step}");
            previous = thumb.top;
        }
        // The end of the range always lands flush with the bottom of the track.
        assert_eq!(thumb(viewport, content, 0.).expect("scrollable").top, 0.);
        let end = thumb(viewport, content, content - viewport).expect("scrollable");
        assert_eq!(
            end.top + end.height,
            viewport,
            "must end flush at the bottom"
        );
    }

    #[test]
    fn geometry_invariants_hold_across_a_sweep_of_sizes_and_positions() {
        for viewport in [1., 8., 23.9, 24., 25., 48., 100., 512., 600., 4_096.] {
            for multiple in 2..64 {
                let content = viewport * multiple as f32;
                for position in [0., 0.25, 0.5, 0.75, 1.] {
                    let offset = (content - viewport) * position;
                    let thumb = thumb(viewport, content, offset)
                        .unwrap_or_else(|| panic!("{viewport}/{content} should be scrollable"));
                    assert!(thumb.top >= 0., "negative top at {viewport}/{content}");
                    assert!(thumb.height > 0., "empty thumb at {viewport}/{content}");
                    assert!(
                        thumb.height <= viewport,
                        "taller than the track at {viewport}/{content}: {}",
                        thumb.height
                    );
                    assert!(
                        (0. ..1.).contains(&thumb.ratio),
                        "ratio out of range at {viewport}/{content}: {}",
                        thumb.ratio
                    );
                    assert!(
                        thumb.top + thumb.height <= viewport + f32::EPSILON * viewport,
                        "thumb escapes the track at {viewport}/{content} offset {offset}"
                    );
                }
            }
        }
    }
}
