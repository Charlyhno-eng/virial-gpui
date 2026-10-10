//! Freedesktop Trash listings and collision-safe restoration.
use crate::domain::models::Entry;
use std::{
    fs, io,
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::MetadataExt,
    },
    path::{Path, PathBuf},
};

fn roots(data: &Path) -> Vec<(PathBuf, Option<PathBuf>)> {
    let mut roots = vec![(data.join("Trash"), None)];
    // Linux exposes the mount table here. macOS has no equivalent file, so only
    // the home Trash is scanned there and mounted volumes are not listed.
    #[cfg(target_os = "linux")]
    {
        let uid = unsafe { libc::getuid() };
        if let Ok(mounts) = fs::read_to_string("/proc/self/mountinfo") {
            for line in mounts.lines() {
                if let Some(mount) = line.split_whitespace().nth(4) {
                    let mount = PathBuf::from(
                        mount
                            .replace("\\040", " ")
                            .replace("\\011", "\t")
                            .replace("\\134", "\\"),
                    );
                    roots.push((mount.join(format!(".Trash-{uid}")), Some(mount.clone())));
                    let shared = mount.join(".Trash");
                    if fs::symlink_metadata(&shared)
                        .is_ok_and(|m| !m.file_type().is_symlink() && m.is_dir())
                    {
                        roots.push((shared.join(uid.to_string()), Some(mount)));
                    }
                }
            }
        }
    }
    roots.sort();
    roots.dedup();
    roots
}

fn valid_root(root: &Path) -> bool {
    [root.to_path_buf(), root.join("files"), root.join("info")]
        .iter()
        .all(|path| {
            fs::symlink_metadata(path).is_ok_and(|m| {
                m.is_dir() && !m.file_type().is_symlink() && m.uid() == unsafe { libc::getuid() }
            })
        })
}

pub(super) fn original(info: &Path, mount: Option<&Path>) -> io::Result<PathBuf> {
    let text = fs::read_to_string(info)?;
    let mut in_section = false;
    let encoded = text
        .lines()
        .find_map(|line| {
            if line.starts_with('[') {
                in_section = line == "[Trash Info]";
            }
            if in_section {
                line.strip_prefix("Path=")
            } else {
                None
            }
        })
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Missing original Trash path"))?;
    let mut bytes = Vec::new();
    let mut iter = encoded.as_bytes().iter().copied();
    while let Some(byte) = iter.next() {
        if byte == b'%' {
            let a = iter.next().and_then(|b| (b as char).to_digit(16));
            let b = iter.next().and_then(|b| (b as char).to_digit(16));
            let (Some(a), Some(b)) = (a, b) else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid Trash path encoding",
                ));
            };
            bytes.push((a * 16 + b) as u8);
        } else {
            bytes.push(byte);
        }
    }
    if bytes.contains(&0) {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let path = PathBuf::from(std::ffi::OsString::from_vec(bytes));
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
        || path.as_os_str().is_empty()
    {
        return Err(io::ErrorKind::InvalidData.into());
    }
    if path.is_absolute() {
        Ok(path)
    } else {
        mount
            .map(|mount| mount.join(path))
            .ok_or_else(|| io::ErrorKind::InvalidData.into())
    }
}

/// Reserve freedesktop metadata before an atomic, same-device trash move.
/// Other devices/platforms retain the existing desktop-service fallback.
#[cfg(target_os = "linux")]
pub(super) fn reserve(
    data: &Path,
    source: &Path,
) -> io::Result<Option<(PathBuf, tempfile::NamedTempFile)>> {
    use std::{
        io::Write,
        os::unix::fs::{DirBuilderExt, PermissionsExt},
    };
    let device = fs::symlink_metadata(source)?.dev();
    let home = data.join("Trash");
    let mut candidates = if fs::metadata(data).is_ok_and(|metadata| metadata.dev() == device) {
        vec![(home.clone(), None)]
    } else {
        roots(data)
    };
    candidates.sort_by_key(|(root, _)| {
        (
            root != &home,
            !root
                .parent()
                .is_some_and(|parent| parent.file_name().is_some_and(|name| name == ".Trash")),
        )
    });
    for (root, mount) in candidates {
        let parent = root.parent().ok_or(io::ErrorKind::InvalidInput)?;
        if mount
            .as_ref()
            .is_some_and(|mount| !source.starts_with(mount))
            || !fs::metadata(parent).is_ok_and(|metadata| metadata.dev() == device)
            || root.starts_with(source)
        {
            continue;
        }
        if parent.file_name().is_some_and(|name| name == ".Trash")
            && !fs::symlink_metadata(parent).is_ok_and(|metadata| {
                metadata.is_dir()
                    && !metadata.is_symlink()
                    && metadata.permissions().mode() & 0o1000 != 0
            })
        {
            continue;
        }
        // Never follow an existing root/subdirectory link or alter its permissions.
        let prepare = || -> io::Result<()> {
            for directory in [&root, &root.join("files"), &root.join("info")] {
                match fs::DirBuilder::new().mode(0o700).create(directory) {
                    Ok(()) => super::sync_directory(directory.parent().unwrap())?,
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error),
                }
                let metadata = fs::symlink_metadata(directory)?;
                if !metadata.is_dir()
                    || metadata.is_symlink()
                    || metadata.uid() != unsafe { libc::getuid() }
                    // The private root prevents other users from reaching its
                    // children. Desktop tools can create files/info as 0775;
                    // those modes do not make a 0700 Trash root accessible.
                    || (directory == &root && metadata.permissions().mode() & 0o077 != 0)
                    || metadata.dev() != device
                {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "Invalid Trash directory",
                    ));
                }
            }
            Ok(())
        };
        if prepare().is_err() {
            continue;
        }
        let root = root.canonicalize()?;
        let location = mount
            .as_ref()
            .and_then(|mount| source.strip_prefix(mount).ok())
            .unwrap_or(source);
        let mut encoded = String::new();
        for byte in location.as_os_str().as_bytes() {
            if byte.is_ascii_alphanumeric() || b"/-._~".contains(byte) {
                encoded.push(*byte as char);
            } else {
                use std::fmt::Write;
                write!(&mut encoded, "%{byte:02X}").unwrap();
            }
        }
        loop {
            let mut info = tempfile::Builder::new()
                .prefix("virial-")
                .suffix(".trashinfo")
                .tempfile_in(root.join("info"))?;
            let name = info.path().file_name().unwrap().as_bytes();
            let destination = root.join("files").join(std::ffi::OsString::from_vec(
                name[..name.len() - ".trashinfo".len()].to_vec(),
            ));
            match fs::symlink_metadata(&destination) {
                Ok(_) => continue,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
            writeln!(
                info,
                "[Trash Info]\nPath={encoded}\nDeletionDate={}",
                chrono::Local::now().format("%Y-%m-%dT%H:%M:%S")
            )?;
            info.as_file().sync_all()?;
            super::sync_directory(&root.join("info"))?;
            return Ok(Some((destination, info)));
        }
    }
    Ok(None)
}

pub fn read(data: &Path) -> io::Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for (root, mount) in roots(data) {
        if !valid_root(&root) {
            continue;
        }
        let files = match fs::read_dir(root.join("files")) {
            Ok(files) => files,
            Err(e)
                if e.kind() == io::ErrorKind::NotFound
                    || (mount.is_some() && e.kind() == io::ErrorKind::PermissionDenied) =>
            {
                continue;
            }
            Err(e) => return Err(e),
        };
        for file in files {
            let file = file?;
            let mut info_name = file.file_name();
            info_name.push(".trashinfo");
            let info = root.join("info").join(info_name);
            // Incomplete or malformed entries are not safe to restore.
            let Ok(original) = original(&info, mount.as_deref()) else {
                continue;
            };
            let metadata = fs::symlink_metadata(file.path())?;
            entries.push(Entry {
                name: original
                    .file_name()
                    .unwrap_or(original.as_os_str())
                    .to_string_lossy()
                    .into(),
                path: file.path(),
                directory: metadata.is_dir(),
                bytes: metadata.is_file().then_some(metadata.len()),
            });
        }
    }
    entries.sort_by_cached_key(|e| (!e.directory, e.name.to_lowercase(), e.path.clone()));
    Ok(entries)
}

/// Empty owned Trash roots, including entries with missing restoration metadata.
pub fn empty(data: &Path) -> io::Result<()> {
    let mut first_error = None;
    for (root, _) in roots(data) {
        if valid_root(&root)
            && let Err(error) = empty_root(&root)
        {
            first_error.get_or_insert(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

fn empty_root(root: &Path) -> io::Result<()> {
    if !valid_root(root) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Invalid Trash directory",
        ));
    }
    // Never follow links to their targets, and retain restoration metadata
    // for any item whose deletion fails.
    let mut first_error = None;
    for file in fs::read_dir(root.join("files"))? {
        let file = file?;
        if let Err(error) = super::operations::remove(&file.path()) {
            first_error.get_or_insert(error);
            continue;
        }
        let mut name = file.file_name();
        name.push(".trashinfo");
        if let Err(error) = fs::remove_file(root.join("info").join(name))
            && error.kind() != io::ErrorKind::NotFound
        {
            first_error.get_or_insert(error);
        }
    }
    // Remove orphan metadata as well, without touching surviving items.
    for info in fs::read_dir(root.join("info"))? {
        let info = info?;
        let name = info.file_name();
        if let Some(name) = name.as_bytes().strip_suffix(b".trashinfo")
            && fs::symlink_metadata(
                root.join("files")
                    .join(std::ffi::OsString::from_vec(name.to_vec())),
            )
            .is_err_and(|error| error.kind() == io::ErrorKind::NotFound)
            && let Err(error) = fs::remove_file(info.path())
        {
            first_error.get_or_insert(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

pub fn restore(data: &Path, paths: &[PathBuf]) -> io::Result<()> {
    let roots = roots(data);
    let mut batch = Vec::new();
    for path in paths {
        let (root, mount) = roots
            .iter()
            .find(|(root, _)| path.parent() == Some(root.join("files").as_path()))
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Not a Trash item"))?;
        if !valid_root(root) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Invalid Trash directory",
            ));
        }
        let mut name = path
            .file_name()
            .ok_or(io::ErrorKind::InvalidInput)?
            .to_os_string();
        name.push(".trashinfo");
        let info = root.join("info").join(name);
        let destination = original(&info, mount.as_deref())?;
        fs::symlink_metadata(path)?;
        match fs::symlink_metadata(&destination) {
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("{} already exists", destination.display()),
                ));
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        if batch.iter().any(|(_, _, d)| d == &destination) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Selected items have the same original path",
            ));
        }
        batch.push((path.clone(), info, destination));
    }
    if batch.is_empty() {
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    if super::undo::restore_trash_moves(data, &batch)? {
        return Ok(());
    }
    let affected = batch
        .iter()
        .flat_map(|(source, info, destination)| [source.clone(), info.clone(), destination.clone()])
        .collect();
    // One selection is one undoable action; partial failures retain their backups.
    super::undo::record(data, affected, || {
        for (source, info, destination) in batch {
            match super::operations::rename(&source, &destination) {
                Ok(()) => {}
                Err(error) if error.raw_os_error() == Some(libc::EXDEV) => {
                    super::operations::copy(&source, &destination)?;
                    super::operations::remove(&source)?;
                }
                Err(error) => return Err(error),
            }
            fs::remove_file(&info)?;
        }
        Ok(())
    })
}

#[cfg(test)]
#[path = "../../tests/infrastructure/trash.rs"]
mod tests;
