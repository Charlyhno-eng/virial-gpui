//! Linux rename undo stores inverse moves, never copies or hashes payloads.
//! Inode identity prevents restoring a replacement; edits travel with the item.
use super::{LIMIT, LOCK, discard_entry, entries, history, invalid, normalized, save};
use crate::infrastructure::{
    operations,
    progress::{Phase, Progress},
    sync_directory, trash as desktop_trash,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{self, Write},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};
use url::Url;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Move {
    source: String,
    destination: String,
    device: u64,
    inode: u64,
    kind: u32,
    info: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    restored_info: Option<TrashInfo>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrashInfo {
    location: String,
    contents: Vec<u8>,
}

impl TrashInfo {
    fn exists(&self) -> io::Result<bool> {
        let location = path(&self.location)?;
        match fs::symlink_metadata(&location) {
            Ok(metadata) if metadata.is_file() && fs::read(location)? == self.contents => Ok(true),
            Ok(_) => Err(io::Error::other(
                "Trash metadata was replaced; journal retained",
            )),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }

    fn remove(&self) -> io::Result<()> {
        if self.exists()? {
            let location = path(&self.location)?;
            fs::remove_file(&location)?;
            sync_directory(location.parent().ok_or_else(invalid)?)?;
        }
        Ok(())
    }

    fn restore(&self) -> io::Result<()> {
        let location = path(&self.location)?;
        if !self.exists()? {
            // Publish complete metadata atomically without replacing another item.
            let mut staging =
                tempfile::NamedTempFile::new_in(location.parent().ok_or_else(invalid)?)?;
            staging.write_all(&self.contents)?;
            staging.as_file().sync_all()?;
            staging
                .persist_noclobber(&location)
                .map_err(|error| error.error)?;
        }
        sync_directory(location.parent().ok_or_else(invalid)?)
    }
}

fn uri(path: &Path) -> io::Result<String> {
    Url::from_file_path(path)
        .map(|url| url.into())
        .map_err(|_| invalid())
}

fn path(value: &str) -> io::Result<PathBuf> {
    Url::parse(value)
        .ok()
        .and_then(|url| url.to_file_path().ok())
        .ok_or_else(invalid)
}

impl Move {
    fn new(source: &Path, destination: &Path, info: Option<&Path>) -> io::Result<Self> {
        let metadata = fs::symlink_metadata(source)?;
        Ok(Self {
            source: uri(source)?,
            destination: uri(destination)?,
            device: metadata.dev(),
            inode: metadata.ino(),
            kind: metadata.mode() & libc::S_IFMT,
            info: info.map(uri).transpose()?,
            restored_info: None,
        })
    }

    fn matches(&self, location: &Path) -> io::Result<bool> {
        match fs::symlink_metadata(location) {
            Ok(metadata) => Ok(metadata.dev() == self.device
                && metadata.ino() == self.inode
                && metadata.mode() & libc::S_IFMT == self.kind),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }

    // Accept both sides after an interruption, but never overwrite a collision.
    fn at_destination(&self) -> io::Result<bool> {
        let source = path(&self.source)?;
        let destination = path(&self.destination)?;
        match (
            fs::symlink_metadata(&source),
            fs::symlink_metadata(&destination),
        ) {
            (Ok(_), Err(error))
                if error.kind() == io::ErrorKind::NotFound && self.matches(&source)? =>
            {
                Ok(false)
            }
            (Err(error), Ok(_))
                if error.kind() == io::ErrorKind::NotFound && self.matches(&destination)? =>
            {
                Ok(true)
            }
            _ => Err(io::Error::other(format!(
                "Cannot restore or resume: {} was replaced, removed, or its name is occupied",
                source.display()
            ))),
        }
    }

    fn info_matches(&self) -> io::Result<bool> {
        let Some(info) = &self.info else {
            return Ok(true);
        };
        let info = path(info)?;
        let root = info.parent().and_then(Path::parent).ok_or_else(invalid)?;
        let parent = root.parent().ok_or_else(invalid)?;
        let mount = if parent.file_name().is_some_and(|name| name == ".Trash") {
            parent.parent()
        } else {
            Some(parent)
        };
        Ok(desktop_trash::original(&info, mount)
            .is_ok_and(|original| path(&self.source).is_ok_and(|source| source == original)))
    }

    fn remove_info(&self) -> io::Result<()> {
        if let Some(info) = &self.info {
            if !fs::symlink_metadata(path(&self.destination)?)
                .is_err_and(|error| error.kind() == io::ErrorKind::NotFound)
            {
                return Ok(());
            }
            let info = path(info)?;
            // A replaced metadata file belongs to somebody else's trash item.
            if self.info_matches()? {
                fs::remove_file(&info)?;
                sync_directory(info.parent().ok_or_else(invalid)?)?;
            }
        }
        Ok(())
    }
}

fn load(directory: &Path) -> io::Result<Option<Vec<Move>>> {
    let file = match File::open(directory.join("moves.json")) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let moves: Vec<Move> = serde_json::from_reader(file)?;
    let mut paths = std::collections::HashSet::new();
    for item in &moves {
        let source = path(&item.source)?;
        let destination = path(&item.destination)?;
        if !source.is_absolute()
            || !destination.is_absolute()
            || !paths.insert(source.clone())
            || !paths.insert(destination.clone())
        {
            return Err(invalid());
        }
        if let Some(info) = &item.info {
            let info = path(info)?;
            let root = destination
                .parent()
                .and_then(Path::parent)
                .ok_or_else(invalid)?;
            let mut name = destination.file_name().ok_or_else(invalid)?.to_os_string();
            name.push(".trashinfo");
            if destination.parent() != Some(root.join("files").as_path())
                || info != root.join("info").join(name)
            {
                return Err(invalid());
            }
        }
        if let Some(info) = &item.restored_info {
            let root = source.parent().and_then(Path::parent).ok_or_else(invalid)?;
            let mut name = source.file_name().ok_or_else(invalid)?.to_os_string();
            name.push(".trashinfo");
            if item.info.is_some()
                || source.parent() != Some(root.join("files").as_path())
                || path(&info.location)? != root.join("info").join(name)
            {
                return Err(invalid());
            }
        }
    }
    Ok(Some(moves))
}

fn publish(
    history: &Path,
    previous: &[(u64, PathBuf)],
    moves: &[Move],
    id: Option<&str>,
    summary: Option<&str>,
) -> io::Result<PathBuf> {
    let number = previous.last().map_or(Ok(1), |(number, _)| {
        number.checked_add(1).ok_or_else(invalid)
    })?;
    let staging = tempfile::Builder::new()
        .prefix(".pending-")
        .tempdir_in(history)?;
    let mut file = File::create(staging.path().join("moves.json"))?;
    serde_json::to_writer(&mut file, moves)?;
    file.sync_all()?;
    for (name, value) in [("operation", id), ("summary", summary)] {
        if let Some(value) = value {
            let mut file = File::create(staging.path().join(name))?;
            file.write_all(value.as_bytes())?;
            file.sync_all()?;
        }
    }
    // This entry is undoable at any point: each atomic move is at either end.
    save(staging.path(), &[], true)?;
    let directory = history.join(number.to_string());
    operations::rename(staging.path(), &directory)?;
    sync_directory(history)?;
    Ok(directory)
}

fn apply(moves: &[Move], progress: Option<&Progress>) -> io::Result<()> {
    if let Some(progress) = progress {
        progress.begin(Phase::Moving, Some(moves.len() as u64));
        progress.totals(moves.len() as u64, 0);
    }
    // Check every source and target before starting a batch.
    for item in moves {
        if let Some(progress) = progress {
            progress.checkpoint()?;
        }
        let moved = item.at_destination()?;
        if !moved && !item.info_matches()? {
            return Err(io::Error::other(
                "Trash metadata changed or is missing; original retained",
            ));
        }
        if let Some(info) = &item.restored_info
            && !info.exists()?
            && !moved
        {
            return Err(io::Error::other(
                "Trash metadata is missing; original retained",
            ));
        }
    }
    for item in moves {
        if let Some(progress) = progress {
            progress.checkpoint()?;
        }
        if !item.at_destination()? {
            if !item.info_matches()? {
                return Err(io::Error::other(
                    "Trash metadata changed or is missing; original retained",
                ));
            }
            let source = path(&item.source)?;
            let destination = path(&item.destination)?;
            operations::rename(&source, &destination)?;
            if !item.matches(&destination)? {
                // Retain the moved item and journal for manual recovery.
                return Err(io::Error::other(
                    "Source was replaced during rename; undo journal retained",
                ));
            }
        }
        // Also flush moves observed after a crash before removing the queue job.
        sync_directory(path(&item.source)?.parent().ok_or_else(invalid)?)?;
        sync_directory(path(&item.destination)?.parent().ok_or_else(invalid)?)?;
        if let Some(info) = &item.restored_info {
            info.remove()?;
        }
        if let Some(progress) = progress {
            progress.advance(1);
            progress.transferred(0, 1);
        }
    }
    Ok(())
}

fn prune(history: &Path) -> io::Result<()> {
    let previous = entries(history)?;
    let count = previous.len().saturating_sub(LIMIT);
    for (_, directory) in previous.into_iter().take(count) {
        discard_entry(&directory)?;
    }
    Ok(())
}

fn finish_trash(
    history: &Path,
    directory: &Path,
    moves: &[Move],
    result: io::Result<()>,
) -> io::Result<bool> {
    // Bind mounts can reject a rename despite sharing a device number. Restore
    // any earlier items before falling back to the desktop trash service.
    if result
        .as_ref()
        .is_err_and(|error| error.raw_os_error() == Some(libc::EXDEV))
    {
        undo(directory)?;
        discard_entry(directory)?;
        return Ok(false);
    }
    let cancelled = result
        .as_ref()
        .is_err_and(|error| error.kind() == io::ErrorKind::Interrupted);
    if cancelled
        && moves
            .iter()
            .all(|item| item.at_destination().is_ok_and(|moved| !moved))
    {
        for item in moves {
            item.remove_info()?;
        }
        discard_entry(directory)?;
    } else if result.is_ok() || cancelled {
        prune(history)?;
    }
    result.map(|_| true)
}

pub(super) fn rename(
    data: &Path,
    source: &Path,
    name: &str,
    progress: Option<&Progress>,
) -> io::Result<()> {
    let _guard = LOCK
        .lock()
        .map_err(|_| io::Error::other("Undo lock unavailable"))?;
    let (history, _lock) = history(data)?;
    let source = normalized(source)?;
    let destination = operations::named_path(source.parent().ok_or_else(invalid)?, name)?;
    if source.starts_with(&history) || history.starts_with(&source) {
        return Err(io::Error::other(
            "Cannot modify the undo history or a folder containing it",
        ));
    }
    if let Some(progress) = progress {
        progress.checkpoint()?;
    }
    let moves = vec![Move::new(&source, &destination, None)?];
    moves[0].at_destination()?;
    let previous = entries(&history)?;
    if let Some((_, latest)) = previous.last() {
        super::load(latest)?;
    }
    let directory = publish(&history, &previous, &moves, None, None)?;
    let result = apply(&moves, progress);
    if result.is_err() && moves[0].matches(&source)? {
        discard_entry(&directory)?;
    } else if result.is_ok() {
        prune(&history)?;
    }
    result
}

pub(super) fn trash(
    data: &Path,
    id: Option<&str>,
    sources: &[PathBuf],
    progress: Option<&Progress>,
) -> io::Result<bool> {
    let _guard = LOCK
        .lock()
        .map_err(|_| io::Error::other("Undo lock unavailable"))?;
    let (history, _lock) = history(data)?;
    let previous = entries(&history)?;
    if let Some((_, directory)) = previous.iter().find(|(_, directory)| {
        id.is_some_and(|id| {
            fs::read_to_string(directory.join("operation")).is_ok_and(|saved| saved == id)
        })
    }) {
        // An older snapshot-based job must continue through the old queue.
        let Some(moves) = load(directory)? else {
            return Ok(false);
        };
        return finish_trash(&history, directory, &moves, apply(&moves, progress));
    }
    if let Some((_, latest)) = previous.last() {
        super::load(latest)?;
    }
    let mut sources = sources
        .iter()
        .map(|source| normalized(source))
        .collect::<io::Result<Vec<_>>>()?;
    sources.sort();
    sources.dedup();
    let folders = sources
        .iter()
        .filter(|path| fs::symlink_metadata(path).is_ok_and(|m| m.is_dir()))
        .cloned()
        .collect::<std::collections::HashSet<_>>();
    sources.retain(|source| {
        !source
            .ancestors()
            .skip(1)
            .any(|parent| folders.contains(parent))
    });
    let journals = data
        .join("virial/operations")
        .canonicalize()
        .unwrap_or(data.join("virial/operations"));
    let mut moves = Vec::new();
    let mut reservations = Vec::new();
    for source in &sources {
        if let Some(progress) = progress {
            progress.checkpoint()?;
        }
        if source.starts_with(&history)
            || history.starts_with(source)
            || source.starts_with(&journals)
            || journals.starts_with(source)
        {
            return Err(io::Error::other(
                "Cannot trash operation journals or undo history",
            ));
        }
        let Some((destination, info)) = desktop_trash::reserve(data, source)? else {
            return Ok(false);
        };
        moves.push(Move::new(source, &destination, Some(info.path()))?);
        reservations.push(info);
    }
    if moves.is_empty() {
        return Ok(true);
    }
    let summary = format!("Trash\t{}", moves.len());
    let directory = publish(&history, &previous, &moves, id, Some(&summary))?;
    // Keep metadata only once the inverse moves are durable.
    for info in reservations {
        info.keep().map_err(|error| error.error)?;
    }
    finish_trash(&history, &directory, &moves, apply(&moves, progress))
}

pub(super) fn restore(data: &Path, batch: &[(PathBuf, PathBuf, PathBuf)]) -> io::Result<bool> {
    let _guard = LOCK
        .lock()
        .map_err(|_| io::Error::other("Undo lock unavailable"))?;
    let (history, _lock) = history(data)?;
    let previous = entries(&history)?;
    if let Some((_, latest)) = previous.last() {
        super::load(latest)?;
    }
    let mut moves = Vec::with_capacity(batch.len());
    let mut locations = std::collections::HashSet::new();
    for (source, info, destination) in batch {
        let source = normalized(source)?;
        let destination = normalized(destination)?;
        for location in [&source, &destination] {
            if !locations.insert(location.clone()) {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "Selected items share a source or original path",
                ));
            }
            if location.starts_with(&history) || history.starts_with(location) {
                return Err(io::Error::other(
                    "Cannot modify the undo history or a folder containing it",
                ));
            }
        }
        // Cross-device restores keep the existing copy-based fallback.
        if fs::symlink_metadata(&source)?.dev()
            != fs::metadata(destination.parent().ok_or_else(invalid)?)?.dev()
        {
            return Ok(false);
        }
        let info = normalized(info)?;
        if !fs::symlink_metadata(&info)?.is_file() {
            return Err(io::Error::other("Invalid Trash metadata"));
        }
        let mut item = Move::new(&source, &destination, None)?;
        item.restored_info = Some(TrashInfo {
            location: uri(&info)?,
            contents: fs::read(&info)?,
        });
        item.at_destination()?;
        moves.push(item);
    }
    let directory = publish(&history, &previous, &moves, None, None)?;
    let result = apply(&moves, None);
    if result
        .as_ref()
        .is_err_and(|error| error.raw_os_error() == Some(libc::EXDEV))
    {
        // A bind mount can reject rename even when device numbers agree.
        undo(&directory)?;
        discard_entry(&directory)?;
        return Ok(false);
    }
    if result.is_ok() {
        prune(&history)?;
    }
    result.map(|_| true)
}

pub(super) fn undo(directory: &Path) -> io::Result<bool> {
    let Some(moves) = load(directory)? else {
        return Ok(false);
    };
    for item in &moves {
        item.at_destination()?;
        if let Some(info) = &item.restored_info {
            info.exists()?;
        }
    }
    for item in moves.iter().rev() {
        if let Some(info) = &item.restored_info {
            info.restore()?;
        }
        if item.at_destination()? {
            let source = path(&item.source)?;
            let destination = path(&item.destination)?;
            operations::rename(&destination, &source)?;
            if !item.matches(&source)? {
                return Err(io::Error::other(
                    "Item was replaced during undo; journal retained",
                ));
            }
        }
        sync_directory(path(&item.source)?.parent().ok_or_else(invalid)?)?;
        sync_directory(path(&item.destination)?.parent().ok_or_else(invalid)?)?;
        item.remove_info()?;
    }
    Ok(true)
}

#[cfg(test)]
#[path = "../../tests/infrastructure/undo_moves.rs"]
mod tests;
