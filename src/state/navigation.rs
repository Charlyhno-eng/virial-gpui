//! Navigation state owned by a pane, and tab ordering owned by the window.
use std::{cell::RefCell, path::PathBuf, rc::Rc};

pub(crate) type FileClipboard = Rc<RefCell<Option<(Vec<PathBuf>, bool)>>>;
pub(crate) const GRID_ROW_HEIGHT: f32 = 112.;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ViewMode {
    #[default]
    List,
    Grid,
}

pub(crate) struct Tabs<T> {
    pub(crate) items: Vec<T>,
    pub(crate) active: usize,
}
impl<T> Tabs<T> {
    pub(crate) fn new(first: T) -> Self {
        Self {
            items: vec![first],
            active: 0,
        }
    }
    pub(crate) fn push(&mut self, item: T) {
        self.items.push(item);
        self.active = self.items.len() - 1;
    }
    pub(crate) fn select(&mut self, index: usize) {
        if index < self.items.len() {
            self.active = index;
        }
    }
    pub(crate) fn cycle(&mut self, backwards: bool) {
        self.active =
            (self.active + if backwards { self.items.len() - 1 } else { 1 }) % self.items.len();
    }
    pub(crate) fn close(&mut self, index: usize) {
        if self.items.len() <= 1 || index >= self.items.len() {
            return;
        }
        self.items.remove(index);
        if index < self.active {
            self.active -= 1;
        }
        self.active = self.active.min(self.items.len() - 1);
    }
}

pub(crate) fn split_ratio(ratio: f32, width: f32) -> f32 {
    let minimum = (200. / width.max(1.)).min(0.5);
    ratio.clamp(minimum, 1. - minimum)
}

pub(crate) fn grid_columns(width: f32) -> usize {
    ((width / 128.).floor() as usize).max(1)
}
pub(crate) fn grid_target(
    key: &str,
    current: Option<usize>,
    count: usize,
    columns: usize,
    page_rows: usize,
) -> Option<usize> {
    if count == 0 {
        return None;
    }
    let Some(current) = current else {
        return Some(0);
    };
    let current = current.min(count - 1);
    Some(match key {
        "left" => current.saturating_sub(1),
        "right" => (current + 1).min(count - 1),
        "up" => {
            if current >= columns {
                current - columns
            } else {
                current
            }
        }
        "down" => {
            if current / columns < (count - 1) / columns {
                (current + columns).min(count - 1)
            } else {
                current
            }
        }
        "home" => 0,
        "end" => count - 1,
        "pageup" => current.saturating_sub(columns * page_rows.max(1)),
        "pagedown" => (current + columns * page_rows.max(1)).min(count - 1),
        _ => return None,
    })
}

impl crate::app::FileManager {
    pub(crate) fn scroll_index(&self, index: usize) -> usize {
        if self.view_mode == ViewMode::Grid {
            index / self.grid_columns
        } else {
            index
        }
    }
    pub(crate) fn set_view_mode(&mut self, mode: ViewMode, cx: &mut gpui::Context<Self>) {
        self.cancel_pending_preview();
        self.marquee = None;
        self.view_mode = mode;
        self.scroll = gpui::UniformListScrollHandle::new();
        self.details_open = false;
        self.preview_expanded = false;
        if let Some(index) = self.selection.primary() {
            self.scroll
                .scroll_to_item(self.scroll_index(index), gpui::ScrollStrategy::Center);
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closing_tabs_preserves_active_identity_and_last_tab() {
        let mut tabs = Tabs::new("a");
        tabs.push("b");
        tabs.push("c");
        tabs.close(0);
        assert_eq!(tabs.items[tabs.active], "c");
        tabs.close(tabs.active);
        assert_eq!(tabs.items[tabs.active], "b");
        tabs.close(0);
        tabs.close(99);
        assert_eq!(tabs.items, ["b"]);
    }
    #[test]
    fn switching_tabs_preserves_independent_navigation() {
        let mut tabs = Tabs::new((PathBuf::from("/first"), vec!["first"], ViewMode::Grid));
        tabs.push((PathBuf::from("/second"), vec!["second"], ViewMode::List));
        tabs.items[1].0.push("child");
        tabs.cycle(false);
        assert_eq!(tabs.active, 0);
        assert_eq!(tabs.items[0].0, PathBuf::from("/first"));
        assert_eq!(tabs.items[0].2, ViewMode::Grid);
        tabs.cycle(true);
        assert_eq!(tabs.items[1].0, PathBuf::from("/second/child"));
        tabs.select(200);
        assert_eq!(tabs.active, 1);
    }
    #[test]
    fn clipboard_is_shared_between_panes_and_tabs() {
        let left: FileClipboard = Default::default();
        let right = left.clone();
        *left.borrow_mut() = Some((vec![PathBuf::from("/source")], true));
        assert_eq!(
            right.borrow().as_ref().unwrap().0,
            [PathBuf::from("/source")]
        );
        *right.borrow_mut() = None;
        assert!(left.borrow().is_none());
    }
    #[test]
    fn split_keeps_both_panes_usable_at_small_window_sizes() {
        assert_eq!(split_ratio(0.1, 1000.), 0.2);
        assert_eq!(split_ratio(0.9, 1000.), 0.8);
        assert_eq!(split_ratio(0.9, 300.), 0.5);
        for ratio in [0.1, 0.5, 0.9] {
            let actual = split_ratio(ratio, 535.);
            assert!(535. * actual >= 199.9);
            assert!(535. * (1. - actual) >= 199.9);
        }
    }
    #[test]
    fn grid_navigation_handles_edges_empty_and_partial_rows() {
        assert_eq!(grid_columns(0.), 1);
        assert_eq!(grid_columns(512.), 4);
        assert_eq!(grid_target("down", None, 0, 4, 2), None);
        assert_eq!(grid_target("down", None, 10, 4, 2), Some(0));
        assert_eq!(grid_target("up", Some(2), 10, 4, 2), Some(2));
        assert_eq!(grid_target("down", Some(7), 10, 4, 2), Some(9));
        assert_eq!(grid_target("right", Some(9), 10, 4, 2), Some(9));
        assert_eq!(grid_target("down", Some(8), 10, 4, 2), Some(8));
        assert_eq!(grid_target("pageup", Some(9), 10, 4, 2), Some(1));
        assert_eq!(grid_target("pagedown", Some(5), 10, 4, 2), Some(9));
    }
}
