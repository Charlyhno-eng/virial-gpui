use super::line_scroll_offset;
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
