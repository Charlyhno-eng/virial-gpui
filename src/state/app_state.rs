use crate::{
    domain::{location::Location, models::Entry, services::History},
    ui::i18n::Language,
};
use gpui::{FocusHandle, Task, UniformListScrollHandle};
use std::path::PathBuf;

pub struct FileManager {
    pub(crate) location: Location,
    pub(crate) language: Language,
    pub(crate) data_home: PathBuf,
    pub(crate) runtime_home: PathBuf,
    pub(crate) home: PathBuf,
    pub(crate) places: Vec<crate::platform::linux::places::Place>,
    pub(crate) entries: Vec<Entry>,
    pub(crate) history: History,
    pub(crate) hidden: bool,
    pub(crate) loading: bool,
    pub(crate) error: Option<String>,
    pub(crate) selected: Option<usize>,
    pub(crate) scroll: UniformListScrollHandle,
    pub(crate) focus: FocusHandle,
    pub(crate) listing: Option<Task<()>>,
    pub(crate) menu: Option<crate::state::actions::Menu>,
    pub(crate) dialog: Option<crate::state::actions::Dialog>,
    pub(crate) clipboard: Option<(PathBuf, bool)>,
    pub(crate) busy: bool,
    pub(crate) titlebar_drag: Option<gpui::Point<gpui::Pixels>>,
}
