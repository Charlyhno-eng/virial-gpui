//! App-owned labels. File URLs preserve paths without modifying user files.
use crate::domain::{models::Entry, tag::Tag};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};
use url::Url;

static WRITE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Default, Clone)]
pub(crate) struct Tags(pub HashMap<PathBuf, Vec<Tag>>);

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    uri: String,
    colors: Vec<Tag>,
}

/// Match operation paths without following the tagged item itself. Resolve the
/// nearest available parent so moved descendants and offline items stay usable.
fn normalized(path: &Path) -> PathBuf {
    for parent in path.ancestors().skip(1) {
        if let Ok(resolved) = parent.canonicalize() {
            return resolved.join(path.strip_prefix(parent).expect("ancestor prefix"));
        }
    }
    path.to_path_buf()
}

impl Tags {
    pub fn colors(&self, path: &Path) -> &[Tag] {
        self.0
            .get(path)
            .or_else(|| self.0.get(&normalized(path)))
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub fn entries(&self, tag: Tag) -> io::Result<Vec<Entry>> {
        let mut entries = Vec::new();
        for (path, colors) in &self.0 {
            if !colors.contains(&tag) {
                continue;
            }
            let metadata = match fs::metadata(path) {
                Ok(metadata) => metadata,
                // Keep unavailable paths in storage for removable drives and restored files.
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied
                    ) =>
                {
                    continue;
                }
                Err(error) => return Err(error),
            };
            entries.push(Entry {
                path: path.clone(),
                name: path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                directory: metadata.is_dir(),
                bytes: metadata.is_file().then_some(metadata.len()),
            });
        }
        crate::state::browser::sort_by_name(&mut entries);
        Ok(entries)
    }
}

pub(crate) fn read(data: &Path) -> io::Result<Tags> {
    let bytes = match fs::read(data.join("virial/tags.json")) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Tags::default()),
        Err(error) => return Err(error),
    };
    let records: Vec<Record> = serde_json::from_slice(&bytes)?;
    let mut tags = Tags::default();
    for record in records {
        let path = Url::parse(&record.uri)
            .ok()
            .and_then(|uri| uri.to_file_path().ok())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Invalid tagged path"))?;
        let colors = tags.0.entry(normalized(&path)).or_default();
        for color in record.colors {
            if !colors.contains(&color) {
                colors.push(color);
            }
        }
    }
    Ok(tags)
}

fn update(data: &Path, edit: impl FnOnce(&mut Tags) -> io::Result<()>) -> io::Result<Tags> {
    let _guard = WRITE_LOCK
        .lock()
        .map_err(|_| io::Error::other("Tags lock unavailable"))?;
    let directory = data.join("virial");
    fs::create_dir_all(&directory)?;
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join("tags.lock"))?;
    super::try_lock_exclusive(&lock, "Tags are being edited in another window")?;
    let mut tags = read(data)?;
    edit(&mut tags)?;
    tags.0.retain(|_, colors| !colors.is_empty());
    let mut records = tags
        .0
        .iter()
        .map(|(path, colors)| {
            let uri = Url::from_file_path(path)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Invalid tagged path"))?;
            Ok(Record {
                uri: uri.into(),
                colors: colors.clone(),
            })
        })
        .collect::<io::Result<Vec<_>>>()?;
    records.sort_by(|a, b| a.uri.cmp(&b.uri));
    let mut temporary = tempfile::NamedTempFile::new_in(&directory)?;
    temporary.write_all(&serde_json::to_vec_pretty(&records)?)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(directory.join("tags.json"))
        .map_err(|error| error.error)?;
    super::sync_directory(&directory)?;
    Ok(tags)
}

/// `None` clears all labels; an explicit state makes batch edits predictable.
pub(crate) fn set(
    data: &Path,
    paths: &[PathBuf],
    tag: Option<Tag>,
    enabled: bool,
) -> io::Result<Tags> {
    update(data, |tags| {
        // Validate the complete batch before changing any labels. Do not follow symlinks.
        for path in paths {
            if !path.is_absolute() || super::archive::is_member(path) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Tags require local files or folders",
                ));
            }
            fs::symlink_metadata(path)?;
        }
        for path in paths {
            let colors = tags.0.entry(normalized(path)).or_default();
            if let Some(tag) = tag {
                colors.retain(|color| *color != tag);
                if enabled {
                    colors.push(tag);
                }
            } else {
                colors.clear();
            }
        }
        Ok(())
    })
}

/// Remap a folder and its tagged descendants after a move, including undo.
pub(crate) fn relocate(data: &Path, source: &Path, destination: &Path) -> io::Result<()> {
    if !data.join("virial/tags.json").exists() {
        return Ok(());
    }
    let source = normalized(source);
    let destination = normalized(destination);
    update(data, |tags| {
        let moved: Vec<_> = tags
            .0
            .keys()
            .filter_map(|path| {
                path.strip_prefix(&source)
                    .ok()
                    .map(|suffix| (path.clone(), destination.join(suffix)))
            })
            .collect();
        for (old, new) in moved {
            let colors = tags.0.remove(&old).unwrap_or_default();
            let target = tags.0.entry(new).or_default();
            for color in colors {
                if !target.contains(&color) {
                    target.push(color);
                }
            }
        }
        Ok(())
    })
    .map(|_| ())
}

#[cfg(test)]
#[path = "../../tests/infrastructure/tags.rs"]
mod tests;
