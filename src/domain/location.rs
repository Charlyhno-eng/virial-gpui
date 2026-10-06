use crate::{domain::services::breadcrumbs, ui::i18n::Language};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Location {
    Directory(PathBuf),
    /// A directory on a connected SSH host. `path` stays remote-absolute.
    Remote {
        host: crate::infrastructure::ssh::HostId,
        path: PathBuf,
    },
    Recent,
    Trash,
    Workspaces,
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
    /// The remote host when browsing a connected server.
    pub fn remote(&self) -> Option<&crate::infrastructure::ssh::HostId> {
        match self {
            Self::Remote { host, .. } => Some(host),
            _ => None,
        }
    }
    pub fn title(&self, home: &Path, language: Language) -> String {
        match self {
            Self::Workspaces => language.text("Workspaces").into(),
            Self::Recent => language.text("Recent").into(),
            Self::Trash => language.text("Trash").into(),
            Self::Directory(path) if path == home => language.text("Home").into(),
            Self::Directory(path) => path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| language.text("File System").into()),
            Self::Remote { path, .. } => path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| language.text("Remote").into()),
        }
    }
    pub fn description(&self, language: Language) -> String {
        match self {
            Self::Workspaces => language.text("Workspaces").into(),
            Self::Directory(path) => path.display().to_string(),
            Self::Remote { host, path } => format!("{host}:{}", path.display()),
            Self::Recent => language.text("Recently opened files").into(),
            Self::Trash => language
                .text("Restore items to their original location")
                .into(),
        }
    }
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Directory(_) => "folder",
            Self::Remote { .. } => "remote",
            Self::Workspaces => "view",
            Self::Recent => "recent",
            Self::Trash => "trash",
        }
    }
    pub fn id(&self) -> gpui::ElementId {
        match self {
            Self::Directory(path) => std::sync::Arc::<Path>::from(path.clone()).into(),
            Self::Remote { path, .. } => std::sync::Arc::<Path>::from(path.clone()).into(),
            Self::Workspaces => "workspaces".into(),
            Self::Recent => "recent-files".into(),
            Self::Trash => "trash-files".into(),
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
