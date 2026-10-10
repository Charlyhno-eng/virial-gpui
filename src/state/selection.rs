//! Selection indices refer to the current, sorted directory listing.
use std::collections::BTreeSet;

#[derive(Clone, Default, Debug)]
pub(crate) struct Selection {
    pub(crate) indices: BTreeSet<usize>,
    pub(crate) focus: Option<usize>,
    pub(crate) anchor: Option<usize>,
    pending_control_toggle: Option<usize>,
}

impl Selection {
    pub(crate) fn primary(&self) -> Option<usize> {
        self.focus
            .filter(|index| self.indices.contains(index))
            .or_else(|| self.indices.first().copied())
    }

    pub(crate) fn click(&mut self, index: usize, control: bool, shift: bool) {
        if shift {
            let anchor = self.anchor.unwrap_or(index);
            if !control {
                self.indices.clear();
            }
            self.indices.extend(anchor.min(index)..=anchor.max(index));
            self.anchor = Some(anchor);
        } else {
            if control {
                if !self.indices.remove(&index) {
                    self.indices.insert(index);
                }
            } else {
                self.indices.clear();
                self.indices.insert(index);
            }
            self.anchor = Some(index);
        }
        self.focus = Some(index);
    }

    /// Add a Ctrl-clicked item immediately, but retain an existing selection
    /// until release so Ctrl-drag can still copy the whole selection.
    pub(crate) fn pointer_down(&mut self, index: usize, control: bool, shift: bool) {
        self.pending_control_toggle = None;
        if control && !shift && self.indices.contains(&index) {
            self.pending_control_toggle = Some(index);
        } else if shift || !self.indices.contains(&index) {
            self.click(index, control, shift);
        }
    }

    pub(crate) fn pointer_click(&mut self, index: usize, control: bool, shift: bool) {
        let pending = self.pending_control_toggle.take();
        if control && !shift {
            if pending == Some(index) {
                self.click(index, true, false);
            }
        } else if !control && !shift {
            self.click(index, false, false);
        }
    }

    pub(crate) fn keyboard_target(&self, key: &str, count: usize, page: usize) -> Option<usize> {
        let last = count.checked_sub(1)?;
        let index = self.focus.unwrap_or(if matches!(key, "up" | "pageup") {
            last
        } else {
            0
        });
        Some(match key {
            "home" => 0,
            "end" => last,
            "up" if self.focus.is_some() => index.saturating_sub(1),
            "down" if self.focus.is_some() => index.saturating_add(1).min(last),
            "pageup" if self.focus.is_some() => index.saturating_sub(page.max(1)),
            "pagedown" if self.focus.is_some() => index.saturating_add(page.max(1)).min(last),
            "up" | "down" | "pageup" | "pagedown" => index.min(last),
            _ => return None,
        })
    }

    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn rectangle(&mut self, baseline: &Self, range: std::ops::Range<usize>) {
        self.indices = baseline.indices.clone();
        self.indices.extend(range);
        self.focus = self.indices.last().copied();
        self.anchor = self.indices.first().copied();
    }
}

#[cfg(test)]
#[path = "../../tests/state/selection.rs"]
mod tests;
