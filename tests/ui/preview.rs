use super::{default_column_width, line_scroll_offset, resized_columns};

#[test]
fn files_are_twice_as_wide_as_folders_across_preview_depths() {
    for parents in 0..30 {
        let file = default_column_width(1200., parents, true, parents + 1);
        let folder = default_column_width(1200., parents, true, parents);
        assert_eq!(file, folder * 2.);
        let start = crate::state::preview::visible_column_start(parents, true);
        assert_eq!(file + folder * (parents + 1 - start) as f32, 1200.);
        let directory = default_column_width(1200., parents, false, parents + 1);
        assert_eq!(directory, 1200. / (parents + 2).min(4) as f32);
    }
}

#[test]
fn resizing_columns_preserves_total_width_and_minimums() {
    for delta in [-1000., -120., 0., 120., 1000.] {
        let (right, left) = resized_columns(300., Some(400.), delta, 800.);
        let left = left.unwrap();
        assert!(right >= 160. && left >= 160.);
        assert_eq!(right + left, 700.);
    }
    assert_eq!(resized_columns(300., None, 1000., 600.), (600., None));
    assert_eq!(resized_columns(300., None, -1000., 600.), (160., None));
}
use gpui::{Bounds, point, px, size};

#[test]
fn repeated_arrow_navigation_keeps_every_line_visible_in_both_directions() {
    // Include the toolbar's vertical offset and the preview's top/bottom padding.
    for height in [90., 450.] {
        let viewport = Bounds::new(point(px(200.), px(100.)), size(px(400.), px(height)));
        let max_offset = px(100. * 18. + 24. - height);
        let mut offset = px(0.);
        for index in (0..100).chain((0..100).rev()) {
            let line = Bounds::new(
                point(px(200.), px(112. + index as f32 * 18.)),
                size(px(800.), px(18.)),
            );
            offset = line_scroll_offset(viewport, line, offset, max_offset);
            assert!(line.top() + offset >= viewport.top());
            assert!(line.bottom() + offset <= viewport.bottom());
        }
    }
}

#[test]
fn visible_lines_do_not_scroll_and_outside_lines_scroll_only_as_needed() {
    let viewport = Bounds::new(point(px(0.), px(100.)), size(px(400.), px(90.)));
    let line = |top| Bounds::new(point(px(0.), px(top)), size(px(800.), px(18.)));
    assert_eq!(
        line_scroll_offset(viewport, line(154.), px(-18.), px(300.)),
        px(-18.)
    );
    assert_eq!(
        line_scroll_offset(viewport, line(190.), px(0.), px(300.)),
        px(-18.)
    );
    assert_eq!(
        line_scroll_offset(viewport, line(112.), px(-18.), px(300.)),
        px(-12.)
    );
    assert_eq!(
        line_scroll_offset(viewport, line(500.), px(0.), px(300.)),
        px(-300.)
    );
}
