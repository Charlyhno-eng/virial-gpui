use crate::{i18n::Language, navigation::breadcrumbs};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Location {
    Directory(PathBuf),
    Recent,
    Network,
}

impl From<PathBuf> for Location {
    fn from(path: PathBuf) -> Self {
        Self::Directory(path)
    }
}
impl From<&str> for Location {
    fn from(path: &str) -> Self {
        Self::Directory(path.into())
    }
}
impl Location {
    pub fn directory(&self) -> Option<&Path> {
        match self {
            Self::Directory(path) => Some(path),
            _ => None,
        }
    }
    pub fn title(&self, home: &Path, language: Language) -> String {
        match self {
            Self::Recent => language.text("Recent").into(),
            Self::Network => language.text("Network").into(),
            Self::Directory(path) if path == home => language.text("Home").into(),
            Self::Directory(path) => path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| language.text("File System").into()),
        }
    }
    pub fn description(&self, language: Language) -> String {
        match self {
            Self::Directory(path) => path.display().to_string(),
            Self::Recent => language.text("Recently opened files").into(),
            Self::Network => language.text("Mounted network shares").into(),
        }
    }
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Directory(_) => "folder",
            Self::Recent => "recent",
            Self::Network => "network",
        }
    }
    pub fn id(&self) -> gpui::ElementId {
        match self {
            Self::Directory(path) => std::sync::Arc::<Path>::from(path.clone()).into(),
            Self::Recent => "recent-files".into(),
            Self::Network => "network-shares".into(),
        }
    }
    pub fn breadcrumbs(&self, home: &Path, language: Language) -> Vec<(String, Self)> {
        match self {
            Self::Directory(path) => breadcrumbs(path, home)
                .into_iter()
                .enumerate()
                .map(|(index, (label, path))| {
                    let label = if index == 0 {
                        language
                            .text(if path == home { "Home" } else { "File System" })
                            .into()
                    } else {
                        label
                    };
                    (label, Self::Directory(path))
                })
                .collect(),
            _ => vec![(self.title(home, language), self.clone())],
        }
    }
}
