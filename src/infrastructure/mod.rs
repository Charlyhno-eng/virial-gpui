pub(crate) mod archive;
pub(crate) mod image_edit;
pub(crate) mod layout;
pub(crate) mod operations;
pub(crate) mod progress;
pub(crate) mod recent;
pub(crate) mod search;
pub(crate) mod storage;
pub(crate) mod undo;
pub(crate) mod workspaces;

pub(crate) mod trash;

#[cfg(test)]
#[path = "../../tests/infrastructure/performance.rs"]
mod performance;
