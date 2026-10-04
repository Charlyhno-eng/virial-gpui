use super::*;
use crate::domain::models::Entry;

#[test]
fn early_enter_waits_for_exact_match_or_completed_scan() {
    let mut picker = GlobalSearch {
        query: "documents".into(),
        results: SearchResults::default(),
        selected: 0,
        selection_moved: false,
        pending_open: true,
        task: None,
        scroll: gpui::UniformListScrollHandle::new(),
    };
    assert!(queued_result(&picker).is_none());
    picker.results.entries.push(Entry {
        path: "/elsewhere/my-documents".into(),
        name: "my-documents".into(),
        directory: true,
        bytes: None,
    });
    assert!(queued_result(&picker).is_none());
    picker.results.entries[0].name = "Documents".into();
    assert!(queued_result(&picker).is_some());
    picker.results.entries[0].name = "my-documents".into();
    picker.results.finished = true;
    assert!(queued_result(&picker).is_some());
    picker.pending_open = false;
    assert!(queued_result(&picker).is_none());
    picker.pending_open = true;
    picker.results.entries.clear();
    assert!(queued_result(&picker).is_none());
}
