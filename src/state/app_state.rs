use crate::{
    domain::{location::Location, models::Entry, services::History},
    ui::i18n::Language,
};
use gpui::{FocusHandle, ScrollHandle, Task, UniformListScrollHandle};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub(crate) struct DirectorySizeTask {
    pub(crate) _task: Task<()>,
    pub(crate) cancelled: Arc<AtomicBool>,
}

impl Drop for DirectorySizeTask {
    fn drop(&mut self) {
        // Dropping a GPUI task alone cannot interrupt synchronous filesystem work.
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

pub struct FileManager {
    pub(crate) location: Location,
    pub(crate) language: Language,
    pub(crate) data_home: PathBuf,
    pub(crate) home: PathBuf,
    pub(crate) places: Vec<crate::platform::linux::places::Place>,
    pub(crate) devices: Vec<crate::platform::linux::devices::Volume>,
    pub(crate) device_error: Option<String>,
    pub(crate) device_monitor: Option<Task<()>>,
    pub(crate) device_generation: usize,
    pub(crate) entries: Vec<Entry>,
    pub(crate) workspaces: Vec<crate::infrastructure::workspaces::Summary>,
    pub(crate) folder_count: usize,
    pub(crate) history: History,
    pub(crate) loading: bool,
    pub(crate) navigation_generation: usize,
    pub(crate) drop_hover: Option<PathBuf>,
    pub(crate) error: Option<String>,
    pub(crate) selection: super::selection::Selection,
    pub(crate) marquee: Option<super::mouse::Marquee>,
    pub(crate) external_drop: Option<super::mouse::ExternalDrop>,
    pub(crate) scroll: UniformListScrollHandle,
    pub(crate) focus: FocusHandle,
    pub(crate) listing: Option<Task<()>>,
    pub(crate) directory_sizes: Option<DirectorySizeTask>,
    pub(crate) menu: Option<crate::state::actions::Menu>,
    pub(crate) rename: Option<crate::state::actions::InlineRename>,
    pub(crate) dialog: Option<crate::state::actions::Dialog>,
    pub(crate) clipboard: Option<(Vec<PathBuf>, bool)>,
    pub(crate) busy: bool,
    pub(crate) extension_filter: String,
    pub(crate) extension_input: gpui::Entity<crate::ui::components::input::NameInput>,
    pub(crate) search_input: gpui::Entity<crate::ui::components::input::NameInput>,
    pub(crate) global_search: Option<super::global_search::GlobalSearch>,
    pub(crate) search_return_focus: bool,
    pub(crate) details_open: bool,
    pub(crate) sidebar_width: f32,
    pub(crate) sidebar_transition: Option<(std::time::Instant, f32, f32)>,
    pub(crate) preview_expanded: bool,
    pub(crate) layout: crate::infrastructure::layout::Layout,
    pub(crate) preview_resize: Option<(gpui::Pixels, f32)>,
    pub(crate) preview_path: Option<PathBuf>,
    pub(crate) preview: super::preview::Preview,
    pub(crate) preview_line: usize,
    pub(crate) preview_scroll: ScrollHandle,
    pub(crate) preview_focused: bool,
    pub(crate) preview_task: Option<Task<()>>,
    pub(crate) opened_archive_files: Vec<crate::infrastructure::archive::Materialized>,
    pub(crate) name_descending: bool,
    pub(crate) titlebar_drag: Option<gpui::Point<gpui::Pixels>>,
}

impl FileManager {
    pub(crate) fn row_height(&self) -> f32 {
        if self.location == Location::Recent {
            crate::ui::theme::RECENT_ROW_HEIGHT
        } else {
            crate::ui::theme::ROW_HEIGHT
        }
    }
}

#[cfg(test)]
#[path = "../../tests/state/app_state.rs"]
mod tests;
