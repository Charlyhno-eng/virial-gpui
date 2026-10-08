//! Portable fallback for "Open with": no application registry on this target.
use crate::ui::i18n::Language;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Application {
    pub name: String,
    pub desktop: PathBuf,
}

pub fn installed(_home: &Path, _language: Language) -> Vec<Application> {
    Vec::new()
}
