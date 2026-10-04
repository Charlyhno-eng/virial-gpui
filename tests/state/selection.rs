use super::*;

#[test]
fn control_toggles_and_shift_keeps_the_original_anchor() {
    let mut selection = Selection::default();
    selection.click(2, false, false);
    selection.click(5, true, false);
    assert_eq!(selection.indices, BTreeSet::from([2, 5]));
    selection.click(5, true, false);
    assert_eq!(selection.primary(), Some(2));
    selection.click(7, false, true);
    assert_eq!(selection.indices, (5..=7).collect());
    selection.click(3, false, true);
    assert_eq!(selection.indices, (3..=5).collect());
    selection.click(9, true, true);
    assert_eq!(selection.indices, (3..=9).collect());
    selection.clear();
    assert_eq!(selection.primary(), None);
}

#[test]
fn shrinking_a_rectangle_restores_only_the_baseline() {
    let mut baseline = Selection::default();
    baseline.click(1, false, false);
    let mut selection = baseline.clone();
    selection.rectangle(&baseline, 3..8);
    assert_eq!(selection.indices, BTreeSet::from([1, 3, 4, 5, 6, 7]));
    selection.rectangle(&baseline, 4..5);
    assert_eq!(selection.indices, BTreeSet::from([1, 4]));
    selection.rectangle(&Selection::default(), 0..0);
    assert_eq!(selection.primary(), None);
}

#[test]
fn keyboard_navigation_handles_boundaries_pages_and_empty_lists() {
    let mut selection = Selection::default();
    assert_eq!(selection.keyboard_target("home", 0, 5), None);
    assert_eq!(selection.keyboard_target("down", 12, 5), Some(0));
    assert_eq!(selection.keyboard_target("up", 12, 5), Some(11));
    selection.click(4, false, false);
    assert_eq!(selection.keyboard_target("pageup", 12, 5), Some(0));
    assert_eq!(selection.keyboard_target("pagedown", 12, 5), Some(9));
    assert_eq!(selection.keyboard_target("home", 12, 5), Some(0));
    let end = selection.keyboard_target("end", 12, 5).unwrap();
    selection.click(end, false, true);
    assert_eq!(selection.indices, (4..12).collect());
    assert_eq!(selection.keyboard_target("down", 12, 5), Some(11));
    assert_eq!(selection.keyboard_target("pagedown", 12, 5), Some(11));
    selection.click(0, false, false);
    assert_eq!(selection.keyboard_target("up", 12, 5), Some(0));
}
