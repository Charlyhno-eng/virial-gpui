//! Bounded, persistent snapshots of filesystem mutations, shared by all windows.
//! History is saved before changing files; undo refuses to discard later edits.
use super::{
    archive,
    operations::{self, Operation},
};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};
#[cfg(unix)]
use std::os::{
    fd::AsRawFd,
    unix::{
        ffi::OsStrExt,
        fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    },
};
use url::Url;

static LOCK: Mutex<()> = Mutex::new(());
const LIMIT: usize = 20;

struct Record {
    path: PathBuf,
    before: String,
    after: String,
}

#[cfg(unix)]
fn os_name_bytes(value: &std::ffi::OsStr) -> Vec<u8> {
    value.as_bytes().to_vec()
}

#[cfg(windows)]
fn os_name_bytes(value: &std::ffi::OsStr) -> Vec<u8> {
    value.to_string_lossy().as_bytes().to_vec()
}

#[cfg(unix)]
fn link_bytes(target: &Path) -> Vec<u8> {
    target.as_os_str().as_bytes().to_vec()
}

#[cfg(windows)]
fn link_bytes(target: &Path) -> Vec<u8> {
    target.to_string_lossy().as_bytes().to_vec()
}

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "Invalid undo history")
}

fn normalized(path: &Path) -> io::Result<PathBuf> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    Ok(parent
        .canonicalize()?
        .join(path.file_name().ok_or_else(invalid)?))
}

fn mutation_path(path: &Path) -> io::Result<PathBuf> {
    if let Some((zip, member)) = archive::split(path)
        && !member.as_os_str().is_empty()
    {
        return zip.canonicalize();
    }
    normalized(path)
}

fn affected(operation: &Operation) -> io::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    match operation {
        Operation::Launch { .. } => {}
        Operation::ImageExport { source, name, edit } => {
            paths.push(mutation_path(&super::image_edit::destination(
                source, name, *edit,
            )?)?);
        }
        Operation::Rename { source, name } => {
            paths.push(mutation_path(source)?);
            paths.push(mutation_path(&operations::named_path(
                source.parent().ok_or_else(invalid)?,
                name,
            )?)?);
        }
        Operation::New {
            directory, name, ..
        } => {
            paths.push(mutation_path(&operations::named_path(directory, name)?)?);
        }
        Operation::Transfer {
            sources,
            directory,
            cut,
        } => {
            for source in sources {
                if *cut {
                    paths.push(mutation_path(source)?);
                }
                let name = source.file_name().ok_or_else(invalid)?;
                paths.push(mutation_path(&directory.join(name))?);
            }
        }
        Operation::Trash(sources) => {
            for source in sources {
                if archive::is_member(source) {
                    return Err(io::Error::new(
                        io::ErrorKind::Unsupported,
                        "ZIP members cannot be moved to the desktop Trash",
                    ));
                }
                paths.push(mutation_path(source)?);
            }
        }
        Operation::Compress(source) => {
            let mut name = source.file_name().ok_or_else(invalid)?.to_os_string();
            name.push(".tar.gz");
            paths.push(normalized(&source.with_file_name(name))?);
        }
    }
    paths.sort();
    paths.dedup();
    let selected = paths.clone();
    paths.retain(|path| {
        !selected
            .iter()
            .any(|parent| parent != path && path.starts_with(parent))
    });
    Ok(paths)
}

/// Hash contents, names, link targets and permissions, without following symlinks.
/// Timestamps and inodes are excluded so restoring a snapshot permits earlier undos.
pub(crate) fn fingerprint(path: &Path) -> io::Result<String> {
    fingerprint_with_progress(path, None)
}

fn fingerprint_with_progress(
    path: &Path,
    progress: Option<&super::progress::Progress>,
) -> io::Result<String> {
    fingerprint_impl(path, progress, true, true)
}

pub(crate) fn fingerprint_controlled(
    path: &Path,
    progress: &super::progress::Progress,
) -> io::Result<String> {
    fingerprint_impl(path, Some(progress), false, true)
}

pub(crate) fn content_fingerprint(
    path: &Path,
    progress: &super::progress::Progress,
) -> io::Result<String> {
    fingerprint_impl(path, Some(progress), false, false)
}

fn fingerprint_impl(
    path: &Path,
    progress: Option<&super::progress::Progress>,
    report: bool,
    permissions: bool,
) -> io::Result<String> {
    fn visit(
        path: &Path,
        hash: &mut Sha256,
        progress: Option<&super::progress::Progress>,
        buffer: &mut Vec<u8>,
        report: bool,
        permissions: bool,
    ) -> io::Result<()> {
        if let Some(progress) = progress {
            progress.checkpoint()?;
        }
        let metadata = fs::symlink_metadata(path)?;
        #[cfg(unix)]
        if permissions {
            hash.update(metadata.permissions().mode().to_le_bytes());
        }
        #[cfg(windows)]
        if permissions {
            // No POSIX mode bits: hash the readonly flag for a stable signature.
            hash.update([u8::from(metadata.permissions().readonly())]);
        }
        if metadata.is_symlink() {
            hash.update(b"link");
            let target = fs::read_link(path)?;
            let bytes = link_bytes(&target);
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(&bytes);
        } else if metadata.is_dir() {
            hash.update(b"directory");
            let mut children = fs::read_dir(path)?.collect::<io::Result<Vec<_>>>()?;
            children.sort_by_key(|entry| entry.file_name());
            hash.update((children.len() as u64).to_le_bytes());
            for child in children {
                let name = child.file_name();
                let bytes = os_name_bytes(&name);
                hash.update((bytes.len() as u64).to_le_bytes());
                hash.update(bytes);
                visit(&child.path(), hash, progress, buffer, report, permissions)?;
            }
        } else if metadata.is_file() {
            hash.update(b"file");
            hash.update(metadata.len().to_le_bytes());
            let mut file = operations::open_regular_file(path)?;
            let opened = file.metadata()?;
            #[cfg(unix)]
            let changed = opened.dev() != metadata.dev()
                || opened.ino() != metadata.ino()
                || opened.len() != metadata.len();
            #[cfg(windows)]
            let changed = opened.len() != metadata.len();
            if changed {
                return Err(io::Error::other("Source changed while reading"));
            }
            // Allocate once for the whole tree, rather than clearing a large
            // stack buffer for every file (including thousands of tiny files).
            if buffer.is_empty() {
                buffer.resize(256 * 1024, 0);
            }
            // Read only the observed length: a constantly growing source must
            // report a change instead of keeping a transfer in preparation forever.
            let mut remaining = metadata.len();
            while remaining > 0 {
                if let Some(progress) = progress {
                    progress.checkpoint()?;
                }
                let limit = remaining.min(buffer.len() as u64) as usize;
                let read = file.read(&mut buffer[..limit])?;
                if read == 0 {
                    return Err(io::Error::other("Source changed while reading"));
                }
                remaining -= read as u64;
                hash.update(&buffer[..read]);
                if report && let Some(progress) = progress {
                    progress.advance(read as u64);
                }
            }
            if file.read(&mut [0u8; 1])? != 0 || file.metadata()?.len() != metadata.len() {
                return Err(io::Error::other("Source changed while reading"));
            }
        } else {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Special files cannot be recorded for undo",
            ));
        }
        if report && let Some(progress) = progress {
            progress.advance(1);
        }
        Ok(())
    }
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok("-".into()),
        result => {
            result?;
        }
    }
    let mut hash = Sha256::new();
    visit(
        path,
        &mut hash,
        progress,
        &mut Vec::new(),
        report,
        permissions,
    )?;
    Ok(format!("{:x}", hash.finalize()))
}

fn save(directory: &Path, records: &[Record], ready: bool) -> io::Result<()> {
    let mut file = tempfile::NamedTempFile::new_in(directory)?;
    writeln!(
        file,
        "virial-undo-1 {}",
        if ready { "ready" } else { "pending" }
    )?;
    for record in records {
        let uri = Url::from_file_path(&record.path).map_err(|_| invalid())?;
        writeln!(file, "{uri}\t{}\t{}", record.before, record.after)?;
    }
    file.as_file().sync_all()?;
    file.persist(directory.join("manifest"))
        .map_err(|error| error.error)?;
    super::sync_directory(directory)
}

fn load(directory: &Path) -> io::Result<Vec<Record>> {
    load_manifest(directory, false)
}

fn load_manifest(directory: &Path, allow_pending: bool) -> io::Result<Vec<Record>> {
    let text = fs::read_to_string(directory.join("manifest"))?;
    let mut lines = text.lines();
    match lines.next() {
        Some("virial-undo-1 ready") => {}
        Some("virial-undo-1 pending") if allow_pending => {}
        Some("virial-undo-1 pending") => {
            return Err(io::Error::other(
                "An interrupted operation has incomplete undo history; its backups have been retained",
            ));
        }
        _ => return Err(invalid()),
    }
    lines
        .map(|line| {
            let fields = line.split('\t').collect::<Vec<_>>();
            let [uri, before, after] = fields.as_slice() else {
                return Err(invalid());
            };
            let valid = |value: &str| {
                value == "-" || (value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()))
            };
            if !valid(before) || !valid(after) {
                return Err(invalid());
            }
            let path = Url::parse(uri)
                .ok()
                .and_then(|uri| uri.to_file_path().ok())
                .ok_or_else(invalid)?;
            Ok(Record {
                path,
                before: (*before).into(),
                after: (*after).into(),
            })
        })
        .collect()
}

fn entries(directory: &Path) -> io::Result<Vec<(u64, PathBuf)>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if let Some(number) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u64>().ok())
        {
            if !entry.file_type()?.is_dir() {
                return Err(invalid());
            }
            entries.push((number, entry.path()));
        }
    }
    entries.sort_by_key(|(number, _)| *number);
    Ok(entries)
}

fn history(data: &Path) -> io::Result<(PathBuf, File)> {
    let directory = data.join("virial/undo");
    #[cfg(unix)]
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&directory)?;
    #[cfg(windows)]
    fs::DirBuilder::new().recursive(true).create(&directory)?;
    #[cfg(unix)]
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(directory.join("lock"))?;
    #[cfg(windows)]
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join("lock"))?;
    // A separate open file description per call serializes other Virial processes too.
    #[cfg(unix)]
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((directory.canonicalize()?, lock))
}

fn sync_snapshot(path: &Path, progress: Option<&super::progress::Progress>) -> io::Result<()> {
    let mut pending = vec![path.to_path_buf()];
    let mut files = Vec::new();
    let mut directories = Vec::new();
    while let Some(path) = pending.pop() {
        if let Some(progress) = progress {
            progress.checkpoint()?;
        }
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            for entry in fs::read_dir(&path)? {
                pending.push(entry?.path());
            }
            directories.push(path);
        } else {
            files.push(path);
        }
    }
    // Undo snapshots must be durable before a move starts. A bounded number of
    // concurrent fsync calls lets the disk batch writes instead of serializing
    // thousands of independent files. Small snapshots avoid thread overhead.
    let workers = files.len().div_ceil(32).clamp(1, 4);
    if workers == 1 {
        for file in &files {
            if let Some(progress) = progress {
                progress.checkpoint()?;
            }
            super::sync_file(file)?;
        }
    } else {
        std::thread::scope(|scope| -> io::Result<()> {
            let tasks = files
                .chunks(files.len().div_ceil(workers))
                .map(|chunk| {
                    scope.spawn(move || -> io::Result<()> {
                        for file in chunk {
                            if let Some(progress) = progress {
                                progress.checkpoint()?;
                            }
                            super::sync_file(file)?;
                        }
                        Ok(())
                    })
                })
                .collect::<Vec<_>>();
            for task in tasks {
                task.join()
                    .map_err(|_| io::Error::other("Snapshot sync worker failed"))??;
            }
            Ok(())
        })?;
    }
    // Persist directory entries only after all file data, children before parents.
    for directory in directories.into_iter().rev() {
        if let Some(progress) = progress {
            progress.checkpoint()?;
        }
        super::sync_directory(&directory)?;
    }
    Ok(())
}

fn snapshot(source: &Path, destination: &Path) -> io::Result<()> {
    snapshot_with_progress(source, destination, None)
}

fn snapshot_with_progress(
    source: &Path,
    destination: &Path,
    progress: Option<&super::progress::Progress>,
) -> io::Result<()> {
    fn timestamps(source: &Path, destination: &Path) -> io::Result<()> {
        let metadata = fs::symlink_metadata(source)?;
        if metadata.is_symlink() {
            return Ok(());
        }
        if metadata.is_dir() {
            for entry in fs::read_dir(source)? {
                let entry = entry?;
                timestamps(&entry.path(), &destination.join(entry.file_name()))?;
            }
        }
        // Windows requires a writable handle to change file times; a read-only
        // File::open would fail with Access Denied there. Unix checks ownership
        // instead, so a read-only handle also covers directories (a writable
        // open fails with EISDIR) and read-only files (EACCES).
        #[cfg(windows)]
        let opened = fs::OpenOptions::new().write(true).open(destination)?;
        #[cfg(unix)]
        let opened = File::open(destination)?;
        opened.set_times(fs::FileTimes::new().set_modified(metadata.modified()?))
    }
    if progress.is_some() {
        operations::copy_with_progress(source, destination, progress)?;
    } else {
        operations::copy(source, destination)?;
    }
    timestamps(source, destination)?;
    let synced = sync_snapshot(destination, progress);
    synced
}

// Snapshots may include read-only folders. Make only our private copies writable
// before discarding them; never change permissions on user paths.
fn discard(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_dir() {
        #[cfg(unix)]
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(metadata.permissions().mode() | 0o700),
        )?;
        #[cfg(windows)]
        {
            let mut permissions = metadata.permissions();
            permissions.set_readonly(false);
            fs::set_permissions(path, permissions)?;
        }
        for entry in fs::read_dir(path)? {
            discard(&entry?.path())?;
        }
        fs::remove_dir(path)
    } else {
        fs::remove_file(path)
    }
}

fn discard_entry(directory: &Path) -> io::Result<()> {
    let parent = directory.parent().ok_or_else(invalid)?;
    let discarded = tempfile::Builder::new()
        .prefix(".discarded-")
        .tempdir_in(parent)?;
    // Remove the entry from the stack atomically before deleting its backups.
    fs::rename(directory, discarded.path().join("entry"))?;
    super::sync_directory(parent)?;
    discard(discarded.path())
}

/// Keep partial successes undoable even when the operation itself reports an error.
pub(crate) fn record<T>(
    data: &Path,
    paths: Vec<PathBuf>,
    action: impl FnOnce() -> io::Result<T>,
) -> io::Result<T> {
    record_with_progress(data, paths, None, action)
}

fn record_with_progress<T>(
    data: &Path,
    paths: Vec<PathBuf>,
    progress: Option<&super::progress::Progress>,
    action: impl FnOnce() -> io::Result<T>,
) -> io::Result<T> {
    let _guard = LOCK
        .lock()
        .map_err(|_| io::Error::other("Undo lock unavailable"))?;
    let (history, _lock) = history(data)?;
    let previous = entries(&history)?;
    if let Some((_, latest)) = previous.last() {
        load(latest)?;
    }
    let number = previous.last().map_or(Ok(1), |(number, _)| {
        number.checked_add(1).ok_or_else(invalid)
    })?;
    let staging = tempfile::Builder::new()
        .prefix(".pending-")
        .tempdir_in(&history)?;
    if let Some(progress) = progress {
        let total = paths
            .iter()
            .map(|path| super::progress::weight(path))
            .collect::<io::Result<Vec<_>>>()?
            .iter()
            .sum::<u64>();
        progress.begin(
            super::progress::Phase::SavingUndo,
            Some(total.saturating_mul(4)),
        );
    }
    let mut records = Vec::new();
    for (index, path) in paths.into_iter().enumerate() {
        if path.starts_with(&history) || history.starts_with(&path) {
            return Err(io::Error::other(
                "Cannot modify the undo history or a folder containing it",
            ));
        }
        let before = fingerprint_with_progress(&path, progress)?;
        if before != "-" {
            let backup = staging.path().join(index.to_string());
            snapshot_with_progress(&path, &backup, progress)?;
            if fingerprint_with_progress(&backup, progress)? != before
                || fingerprint_with_progress(&path, progress)? != before
            {
                return Err(io::Error::other(
                    "File changed while recording undo history; retry",
                ));
            }
        }
        records.push(Record {
            path,
            before,
            after: "-".into(),
        });
    }
    save(staging.path(), &records, false)?;
    let directory = history.join(number.to_string());
    fs::rename(staging.path(), &directory)?;
    super::sync_directory(&history)?;
    let result = action();
    if let Some(progress) = progress {
        progress.begin(super::progress::Phase::Finishing, None);
        let total = records
            .iter()
            .map(|record| super::progress::weight(&record.path))
            .collect::<io::Result<Vec<_>>>()?
            .iter()
            .sum();
        progress.begin(super::progress::Phase::Finishing, Some(total));
    }
    for record in &mut records {
        record.after = fingerprint_with_progress(&record.path, progress)?;
    }
    save(&directory, &records, true)?;
    if records.iter().all(|record| record.before == record.after) {
        discard_entry(&directory)?;
    } else {
        let oldest = previous.len().saturating_add(1).saturating_sub(LIMIT);
        for (_, path) in previous.into_iter().take(oldest) {
            discard_entry(&path)?;
        }
    }
    result
}

/// A queued operation records its intended final states before any mutation.
/// Recovery accepts only original or planned contents, never arbitrary later edits.
pub(crate) fn durable(
    data: &Path,
    id: &str,
    planned: Vec<(PathBuf, String)>,
    summary: &str,
    progress: &super::progress::Progress,
    action: impl FnOnce() -> io::Result<()>,
) -> io::Result<()> {
    let _guard = LOCK
        .lock()
        .map_err(|_| io::Error::other("Undo lock unavailable"))?;
    let (history, _lock) = history(data)?;
    let previous = entries(&history)?;
    let existing = previous
        .iter()
        .find(|(_, path)| fs::read_to_string(path.join("operation")).is_ok_and(|key| key == id));
    let (directory, mut records) = if let Some((_, directory)) = existing {
        let records = load_manifest(directory, true)?;
        if load(directory).is_ok() {
            // The action and its undo entry were already finalized before the crash.
            progress.begin(super::progress::Phase::Finishing, None);
            return Ok(());
        }
        if records.len() != planned.len()
            || records
                .iter()
                .zip(&planned)
                .any(|(record, (path, after))| record.path != *path || record.after != *after)
        {
            return Err(io::Error::other(
                "Recovery plan does not match saved undo history",
            ));
        }
        for (index, record) in records.iter().enumerate() {
            let current = fingerprint(&record.path)?;
            if current != record.before && current != record.after {
                return Err(io::Error::other(format!(
                    "Cannot resume: {} changed after interruption",
                    record.path.display()
                )));
            }
            if record.before != "-"
                && fingerprint(&directory.join(index.to_string()))? != record.before
            {
                return Err(io::Error::other("Cannot resume: undo backup is damaged"));
            }
        }
        (directory.clone(), records)
    } else {
        if let Some((_, latest)) = previous.last() {
            load(latest)?;
        }
        let number = previous.last().map_or(Ok(1), |(number, _)| {
            number.checked_add(1).ok_or_else(invalid)
        })?;
        let staging = tempfile::Builder::new()
            .prefix(".pending-")
            .tempdir_in(&history)?;
        let total = planned
            .iter()
            .map(|(path, _)| super::progress::weight(path))
            .collect::<io::Result<Vec<_>>>()?
            .into_iter()
            .sum::<u64>();
        progress.begin(
            super::progress::Phase::SavingUndo,
            Some(total.saturating_mul(4)),
        );
        let mut records = Vec::new();
        for (index, (path, after)) in planned.into_iter().enumerate() {
            if path.starts_with(&history) || history.starts_with(&path) {
                return Err(io::Error::other(
                    "Cannot modify the undo history or a folder containing it",
                ));
            }
            let before = fingerprint_with_progress(&path, Some(progress))?;
            if before != "-" {
                let backup = staging.path().join(index.to_string());
                snapshot_with_progress(&path, &backup, Some(progress))?;
                let backup_now = fingerprint_with_progress(&backup, Some(progress))?;
                let source_now = fingerprint_with_progress(&path, Some(progress))?;
                if backup_now != before || source_now != before {
                    return Err(io::Error::other(
                        "File changed while recording undo history; retry",
                    ));
                }
            }
            records.push(Record {
                path,
                before,
                after,
            });
        }
        save(staging.path(), &records, false)?;
        for (name, value) in [("operation", id), ("summary", summary)] {
            let mut file = File::create(staging.path().join(name))?;
            file.write_all(value.as_bytes())?;
            file.sync_all()?;
        }
        let directory = history.join(number.to_string());
        if let Err(error) = fs::rename(staging.path(), &directory) {
                return Err(error);
        }
        if let Err(error) = super::sync_directory(&history) {
                return Err(error);
        }
        (directory, records)
    };
    let result = action();
    // I/O failures retain a pending plan and partial copies for Retry. Only an
    // explicit cancellation finalizes partial successes as a ready undo batch.
    if result
        .as_ref()
        .is_err_and(|error| error.kind() != io::ErrorKind::Interrupted)
    {
        return result;
    }

    progress.begin(super::progress::Phase::Finishing, None);
    let total = records
        .iter()
        .map(|record| super::progress::weight(&record.path))
        .collect::<io::Result<Vec<_>>>()?
        .into_iter()
        .sum();
    progress.begin(super::progress::Phase::Finishing, Some(total));
    for record in &mut records {
        let current = fingerprint_with_progress(&record.path, Some(progress))?;
        if current != record.before && current != record.after {
            return Err(io::Error::other(format!(
                "Contents changed during operation: {}; journal and backups retained",
                record.path.display()
            )));
        }
        record.after = current;
    }
    save(&directory, &records, true)?;
    // Keep even a no-op entry until the queue journal is removed. Its operation
    // marker prevents replay in the crash window between finalization and removal.
    if records.iter().any(|record| record.before != record.after) {
        let oldest = entries(&history)?.len().saturating_sub(LIMIT);
        for (_, path) in previous.into_iter().take(oldest) {
            discard_entry(&path)?;
        }
    }
    result
}

pub(crate) fn discard_noop(data: &Path, id: &str) -> io::Result<()> {
    let _guard = LOCK
        .lock()
        .map_err(|_| io::Error::other("Undo lock unavailable"))?;
    let (history, _lock) = history(data)?;
    for (_, directory) in entries(&history)? {
        if fs::read_to_string(directory.join("operation")).is_ok_and(|key| key == id) {
            let records = load(&directory)?;
            if records.iter().all(|record| record.before == record.after) {
                discard_entry(&directory)?;
            }
            break;
        }
    }
    Ok(())
}

// Cleanup is allowed only after the durable manifest reached its ready state.
pub(crate) fn finalized(data: &Path, id: &str) -> io::Result<bool> {
    let _guard = LOCK
        .lock()
        .map_err(|_| io::Error::other("Undo lock unavailable"))?;
    let (history, _lock) = history(data)?;
    for (_, directory) in entries(&history)? {
        if fs::read_to_string(directory.join("operation")).is_ok_and(|key| key == id) {
            return Ok(load(&directory).is_ok());
        }
    }
    Ok(false)
}

fn entry_key(directory: &Path) -> io::Result<String> {
    match fs::read_to_string(directory.join("operation")) {
        Ok(id) => Ok(id),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(directory
            .file_name()
            .ok_or_else(invalid)?
            .to_string_lossy()
            .into_owned()),
        Err(error) => Err(error),
    }
}

pub(crate) fn latest_summary(data: &Path) -> io::Result<Option<(String, String)>> {
    let _guard = LOCK
        .lock()
        .map_err(|_| io::Error::other("Undo lock unavailable"))?;
    let (history, _lock) = history(data)?;
    let Some((_, directory)) = entries(&history)?.pop() else {
        return Ok(None);
    };
    load(&directory)?;
    match fs::read_to_string(directory.join("summary")) {
        Ok(summary) => Ok(Some((entry_key(&directory)?, summary))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
pub fn execute(data: &Path, operation: Operation) -> io::Result<Option<archive::Materialized>> {
    execute_with_progress(data, operation, None)
}

pub fn execute_with_progress(
    data: &Path,
    operation: Operation,
    progress: Option<&super::progress::Progress>,
) -> io::Result<Option<archive::Materialized>> {
    if matches!(operation, Operation::Launch { .. }) {
        return operations::execute(operation);
    }
    let paths = affected(&operation)?;
    if progress.is_some() {
        record_with_progress(data, paths, progress, || {
            operations::execute_with_progress(operation, progress)
        })
    } else {
        record(data, paths, || operations::execute(operation))
    }
}

struct StagedSnapshot(PathBuf);

impl Drop for StagedSnapshot {
    fn drop(&mut self) {
        let _ = discard(&self.0);
    }
}

fn temporary_path(parent: &Path) -> io::Result<PathBuf> {
    let path = tempfile::Builder::new()
        .prefix(".virial-undo-")
        .tempdir_in(parent)?
        .keep();
    fs::remove_dir(&path)?;
    Ok(path)
}

/// Stage siblings of the target, so even read-only directories can be renamed
/// without changing their parent or temporarily changing their permissions.
fn restore(path: &Path, backup: &Path, before: &str, after: &str) -> io::Result<()> {
    let parent = path.parent().ok_or_else(invalid)?;
    let replacement = StagedSnapshot(temporary_path(parent)?);
    // Parked user data is retained on errors unless rollback succeeds.
    let parked = temporary_path(parent)?;
    if before != "-" {
        snapshot(backup, &replacement.0)?;
    }
    if fingerprint(path)? != after {
        return Err(io::Error::other("File changed during undo; retry"));
    }
    if after != "-" {
        operations::rename(path, &parked)?;
        // Recheck the actual parked item before discarding it, in case it changed
        // between validation and rename.
        let parked_state = fingerprint(&parked);
        if !parked_state.as_ref().is_ok_and(|state| state == after) {
            if operations::rename(&parked, path).is_err() {
                return Err(io::Error::other(format!(
                    "File changed during undo; original retained in {}",
                    parked.display()
                )));
            }
            return Err(io::Error::other("File changed during undo; retry"));
        }
    }
    if before != "-"
        && let Err(error) = operations::rename(&replacement.0, path)
    {
        if after != "-" && operations::rename(&parked, path).is_err() {
            return Err(io::Error::other(format!(
                "Undo failed: {error}; original retained in {}",
                parked.display()
            )));
        }
        return Err(error);
    }
    super::sync_directory(parent)?;
    if after != "-" {
        discard(&parked)?;
    }
    Ok(())
}

/// A removed source and a new destination with identical contents describe a
/// move. Rename it back when possible to preserve its inode and hard links.
fn restore_moves(records: &[Record]) -> io::Result<()> {
    for source in records {
        if source.before == "-" || source.after != "-" || fingerprint(&source.path)? != "-" {
            continue;
        }
        let candidates = records
            .iter()
            .filter(|target| target.before == "-" && target.after == source.before)
            .collect::<Vec<_>>();
        let target = candidates
            .iter()
            .find(|target| target.path.file_name() == source.path.file_name())
            .copied()
            .or_else(|| (candidates.len() == 1).then(|| candidates[0]));
        let Some(target) = target else {
            continue;
        };
        if fingerprint(&target.path)? != target.after {
            continue;
        }
        match operations::rename(&target.path, &source.path) {
            Ok(()) => {}
            #[cfg(unix)]
            Err(error) if error.raw_os_error() == Some(libc::EXDEV) => continue,
            #[cfg(windows)]
            Err(error) if error.raw_os_error() == Some(17) => continue,
            Err(error) => return Err(error),
        }
        if fingerprint(&source.path)
            .as_ref()
            .is_ok_and(|state| *state == source.before)
        {
            super::sync_directory(source.path.parent().ok_or_else(invalid)?)?;
            super::sync_directory(target.path.parent().ok_or_else(invalid)?)?;
        } else {
            if operations::rename(&source.path, &target.path).is_err() {
                return Err(io::Error::other(format!(
                    "File changed during undo; retained in {}",
                    source.path.display()
                )));
            }
            return Err(io::Error::other("File changed during undo; retry"));
        }
    }
    Ok(())
}

/// Returns false for an empty history. A failed undo retains the entry for retry.
pub fn undo(data: &Path) -> io::Result<bool> {
    undo_impl(data, None)
}

pub(crate) fn undo_expected(data: &Path, expected: &str) -> io::Result<bool> {
    undo_impl(data, Some(expected))
}

fn undo_impl(data: &Path, expected: Option<&str>) -> io::Result<bool> {
    let _guard = LOCK
        .lock()
        .map_err(|_| io::Error::other("Undo lock unavailable"))?;
    let (history, _lock) = history(data)?;
    let Some((_, directory)) = entries(&history)?.pop() else {
        return Ok(false);
    };
    if expected.is_some_and(|expected| entry_key(&directory).map_or(true, |key| key != expected)) {
        return Err(io::Error::other("Undo history changed; press Ctrl+Z again"));
    }
    let records = load(&directory)?;
    // Check the entire batch and its backups before restoring any item.
    for (index, record) in records.iter().enumerate() {
        if record.before == record.after {
            continue;
        }
        let current = fingerprint(&record.path)?;
        if current != record.after && current != record.before {
            return Err(io::Error::other(format!(
                "Cannot undo: {} changed since the operation",
                record.path.display()
            )));
        }
        if current != record.before
            && record.before != "-"
            && fingerprint(&directory.join(index.to_string()))? != record.before
        {
            return Err(io::Error::other(
                "Cannot undo: a saved backup is missing or damaged",
            ));
        }
    }
    restore_moves(&records)?;
    for (index, record) in records.iter().enumerate() {
        // An earlier attempt may have restored part of a batch before an I/O failure.
        if record.before != record.after && fingerprint(&record.path)? != record.before {
            restore(
                &record.path,
                &directory.join(index.to_string()),
                &record.before,
                &record.after,
            )?;
        }
    }
    discard_entry(&directory)?;
    Ok(true)
}

#[cfg(all(test, unix))]
#[path = "../../tests/infrastructure/undo.rs"]
mod tests;
