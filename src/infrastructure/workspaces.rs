//! Persistent logical groups of folders and shallow, deduplicated summaries.
use crate::{domain::models::Entry, infrastructure::storage::read_directory};
use std::{
    collections::HashSet,
    fs, io,
    path::{Path, PathBuf},
    sync::Mutex,
    time::SystemTime,
};
use url::{Url, form_urlencoded};

static WRITE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Workspace {
    pub name: String,
    pub folders: Vec<PathBuf>,
}

pub(crate) struct Summary {
    pub workspace: Workspace,
    pub files: usize,
    pub directories: usize,
    pub recent: Vec<(SystemTime, Entry)>,
    pub unavailable: Vec<PathBuf>,
}

pub(crate) enum Edit {
    Add {
        name: String,
        folder: Option<PathBuf>,
    },
    Remove(String),
    RemoveFolder {
        name: String,
        folder: PathBuf,
    },
}

pub(crate) fn read(data: &Path) -> io::Result<Vec<Workspace>> {
    let text = match fs::read_to_string(data.join("virial/workspaces")) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        result => result?,
    };
    let invalid = || {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid workspace configuration",
        )
    };
    let mut workspaces = Vec::new();
    let mut names = HashSet::new();
    for line in text.lines().filter(|line| !line.is_empty()) {
        let mut fields = form_urlencoded::parse(line.as_bytes());
        let (key, name) = fields.next().ok_or_else(invalid)?;
        if key != "name" || name.trim().is_empty() || !names.insert(name.to_string()) {
            return Err(invalid());
        }
        let mut folders = Vec::new();
        for (key, value) in fields {
            if key != "folder" {
                return Err(invalid());
            }
            let folder = Url::parse(&value)
                .ok()
                .and_then(|url| url.to_file_path().ok())
                .ok_or_else(invalid)?;
            if !folders.contains(&folder) {
                folders.push(folder);
            }
        }
        workspaces.push(Workspace {
            name: name.into_owned(),
            folders,
        });
    }
    Ok(workspaces)
}

pub(crate) fn edit(data: &Path, edit: Edit) -> io::Result<()> {
    let _guard = WRITE_LOCK
        .lock()
        .map_err(|_| io::Error::other("Workspace lock unavailable"))?;
    let mut workspaces = read(data)?;
    match edit {
        Edit::Add { name, folder } => {
            let name = name.trim();
            if name.is_empty() || name.chars().any(char::is_control) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Invalid workspace name",
                ));
            }
            let folder = folder
                .map(|path| {
                    let path = path.canonicalize()?;
                    if !path.is_dir() {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "Workspace roots must be folders",
                        ));
                    }
                    Ok(path)
                })
                .transpose()?;
            let index = if let Some(index) = workspaces
                .iter()
                .position(|workspace| workspace.name == name)
            {
                index
            } else {
                workspaces.push(Workspace {
                    name: name.into(),
                    folders: Vec::new(),
                });
                workspaces.len() - 1
            };
            if let Some(folder) = folder
                && !workspaces[index].folders.contains(&folder)
            {
                workspaces[index].folders.push(folder);
            }
        }
        Edit::Remove(name) => workspaces.retain(|workspace| workspace.name != name),
        Edit::RemoveFolder { name, folder } => {
            if let Some(workspace) = workspaces
                .iter_mut()
                .find(|workspace| workspace.name == name)
            {
                workspace.folders.retain(|path| path != &folder);
            }
        }
    }
    let mut text = String::new();
    for workspace in workspaces {
        let mut line = form_urlencoded::Serializer::new(String::new());
        line.append_pair("name", &workspace.name);
        for folder in workspace.folders {
            let uri = Url::from_file_path(folder)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Invalid folder path"))?;
            line.append_pair("folder", uri.as_str());
        }
        text.push_str(&line.finish());
        text.push('\n');
    }
    let directory = data.join("virial");
    fs::create_dir_all(&directory)?;
    let temporary = directory.join(format!("workspaces.{}.tmp", std::process::id()));
    fs::write(&temporary, text)?;
    fs::rename(&temporary, directory.join("workspaces"))
}

pub(crate) fn summaries(data: &Path, hidden: bool) -> io::Result<Vec<Summary>> {
    Ok(read(data)?
        .into_iter()
        .map(|workspace| {
            let mut summary = Summary {
                workspace,
                files: 0,
                directories: 0,
                recent: Vec::new(),
                unavailable: Vec::new(),
            };
            let mut seen = HashSet::new();
            for folder in &summary.workspace.folders {
                match read_directory(folder, hidden) {
                    Ok(entries) => {
                        for entry in entries {
                            if !seen.insert(entry.path.clone()) {
                                continue;
                            }
                            if entry.directory {
                                summary.directories += 1;
                            } else {
                                summary.files += 1;
                                if let Ok(modified) = fs::metadata(&entry.path)
                                    .and_then(|metadata| metadata.modified())
                                {
                                    summary.recent.push((modified, entry));
                                    summary.recent.sort_by(|a, b| {
                                        b.0.cmp(&a.0).then_with(|| a.1.path.cmp(&b.1.path))
                                    });
                                    summary.recent.truncate(5);
                                }
                            }
                        }
                    }
                    Err(_) => summary.unavailable.push(folder.clone()),
                }
            }
            summary
        })
        .collect())
}

#[cfg(test)]
#[path = "../../tests/infrastructure/workspaces.rs"]
mod tests;
