//! Browser display preferences and name ordering.
use crate::{app::FileManager, domain::models::Entry, state::selection::Selection};
use gpui::{Context, ScrollStrategy};

pub(crate) fn matches_extension(entry: &Entry, extension: &str) -> bool {
    extension.is_empty()
        || entry.directory
        || entry
            .path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.to_lowercase() == extension)
}

/// Match names or extensions in the current local listing.
pub(crate) fn matches_file_filter(entry: &Entry, query: &str) -> bool {
    query.is_empty()
        || entry.name.to_lowercase().contains(query)
        || matches_extension(entry, query.trim_start_matches('.')) && !entry.directory
}

pub(crate) fn sort_entries(entries: &mut [Entry], selection: &mut Selection, descending: bool) {
    let paths: std::collections::HashSet<_> = selection
        .indices
        .iter()
        .filter_map(|index| entries.get(*index).map(|entry| entry.path.clone()))
        .collect();
    let focus = selection
        .focus
        .and_then(|index| entries.get(index))
        .map(|entry| entry.path.clone());
    let anchor = selection
        .anchor
        .and_then(|index| entries.get(index))
        .map(|entry| entry.path.clone());
    sort_by_name(entries);
    apply_name_direction(entries, descending);
    selection.indices = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| paths.contains(&entry.path))
        .map(|(index, _)| index)
        .collect();
    selection.focus = entries
        .iter()
        .position(|entry| Some(&entry.path) == focus.as_ref());
    selection.anchor = entries
        .iter()
        .position(|entry| Some(&entry.path) == anchor.as_ref());
}

pub(crate) fn sort_by_name(entries: &mut [Entry]) {
    entries.sort_by_cached_key(|entry| {
        (
            !entry.directory,
            entry.name.to_lowercase(),
            entry.path.clone(),
        )
    });
}

pub(crate) fn apply_name_direction(entries: &mut [Entry], descending: bool) {
    if descending {
        let folders = entries.partition_point(|entry| entry.directory);
        entries[..folders].reverse();
        entries[folders..].reverse();
    }
}

impl FileManager {
    pub(crate) fn toggle_name_sort(&mut self, cx: &mut Context<Self>) {
        // Size results refer to indices, so cancel them before reordering.
        self.directory_sizes = None;
        self.marquee = None;
        self.name_descending = !self.name_descending;
        sort_entries(&mut self.entries, &mut self.selection, self.name_descending);
        if let Some(index) = self.selection.primary() {
            self.scroll.scroll_to_item(index, ScrollStrategy::Center);
        }
        self.load_directory_sizes(cx);
        cx.notify();
    }
}

#[cfg(test)]
#[path = "../../tests/state/browser.rs"]
mod tests;
