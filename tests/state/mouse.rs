use super::*;

#[test]
fn rectangle_selects_intersecting_rows_and_clips_to_listing() {
    assert_eq!(
        rectangle_rows(ROW_HEIGHT * 2. + 1., ROW_HEIGHT * 4., 20, ROW_HEIGHT),
        2..4
    );
    assert_eq!(rectangle_rows(-20., ROW_HEIGHT + 1., 20, ROW_HEIGHT), 0..2);
    assert_eq!(rectangle_rows(0., ROW_HEIGHT * 100., 10, ROW_HEIGHT), 0..10);
    assert_eq!(
        rectangle_rows(ROW_HEIGHT * 20., ROW_HEIGHT * 21., 10, ROW_HEIGHT),
        10..10
    );
    assert_eq!(rectangle_rows(10., 10., 10, ROW_HEIGHT), 0..0);
    assert_eq!(
        rectangle_rows(
            crate::ui::theme::RECENT_ROW_HEIGHT * 2.,
            crate::ui::theme::RECENT_ROW_HEIGHT * 4.,
            20,
            crate::ui::theme::RECENT_ROW_HEIGHT,
        ),
        2..4
    );
}

#[test]
fn external_drop_coordinates_account_for_x11_scaling_only() {
    let position = point(px(450.), px(330.));
    assert_eq!(
        external_position(position, 1.5, "X11"),
        point(px(300.), px(220.))
    );
    assert_eq!(external_position(position, 1., "X11"), position);
    assert_eq!(external_position(position, 1.5, "Wayland"), position);
}
