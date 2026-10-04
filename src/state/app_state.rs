use crate::{
    domain::{location::Location, models::Entry, services::History},
    ui::i18n::Language,
};
use gpui::{FocusHandle, Task, UniformListScrollHandle};
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
    pub(crate) runtime_home: PathBuf,
    pub(crate) home: PathBuf,
    pub(crate) places: Vec<crate::platform::linux::places::Place>,
    pub(crate) entries: Vec<Entry>,
    pub(crate) workspaces: Vec<crate::infrastructure::workspaces::Summary>,
    pub(crate) folder_count: usize,
    pub(crate) history: History,
    pub(crate) hidden: bool,
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
    pub(crate) dialog: Option<crate::state::actions::Dialog>,
    pub(crate) clipboard: Option<(Vec<PathBuf>, bool)>,
    pub(crate) busy: bool,
    pub(crate) search_input: gpui::Entity<crate::ui::components::input::NameInput>,
    pub(crate) global_search: Option<super::global_search::GlobalSearch>,
    pub(crate) details_open: bool,
    pub(crate) preview_expanded: bool,
    pub(crate) preview_path: Option<PathBuf>,
    pub(crate) preview: super::preview::Preview,
    pub(crate) preview_task: Option<Task<()>>,
    pub(crate) compact_view: bool,
    pub(crate) display_menu: bool,
    pub(crate) folder_details: bool,
    pub(crate) name_descending: bool,
    pub(crate) titlebar_drag: Option<gpui::Point<gpui::Pixels>>,
}

#[cfg(test)]
#[path = "../../tests/state/app_state.rs"]
mod tests;
