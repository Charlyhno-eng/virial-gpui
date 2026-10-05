//! Durable local transfer/Trash jobs. Destinations publish atomically from private
//! staging, so a crash cannot expose a half-copied tree or replay a finished job.
use super::{
    operations::{self, Operation},
    progress::{Conflict, Phase, Progress, Resolution},
    undo,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom},
    os::{
        fd::AsRawFd,
        unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt, symlink},
    },
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use url::Url;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    version: u8,
    pub id: String,
    queued_at: u64,
    sources: Vec<String>,
    directory: Option<String>,
    cut: bool,
    pub verify: bool,
    plan: Option<Vec<Item>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Item {
    source: String,
    destination: Option<String>,
    staging: Option<String>,
    parked: Option<String>,
    hash: String,
    files: u64,
    bytes: u64,
}

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "Invalid operation journal")
}
fn uri(path: &Path) -> io::Result<String> {
    Url::from_file_path(path)
        .map(|url| url.to_string())
        .map_err(|_| invalid())
}
fn path(uri: &str) -> io::Result<PathBuf> {
    Url::parse(uri)
        .ok()
        .and_then(|url| url.to_file_path().ok())
        .ok_or_else(invalid)
}
fn root(data: &Path) -> io::Result<PathBuf> {
    let root = data.join("virial/operations");
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&root)?;
    root.canonicalize()
}
fn journal(data: &Path, job: &Job) -> io::Result<PathBuf> {
    if job.id.is_empty()
        || !job
            .id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err(invalid());
    }
    Ok(root(data)?.join(format!("{}.json", job.id)))
}
fn save(data: &Path, job: &Job) -> io::Result<()> {
    let destination = journal(data, job)?;
    let mut file = tempfile::NamedTempFile::new_in(destination.parent().unwrap())?;
    serde_json::to_writer(&mut file, job)?;
    file.as_file().sync_all()?;
    file.persist(destination).map_err(|error| error.error)?;
    File::open(root(data)?)?.sync_all()
}

pub fn supported(operation: &Operation) -> bool {
    match operation {
        Operation::Transfer {
            sources, directory, ..
        } => {
            super::archive::split(directory).is_none()
                && sources
                    .iter()
                    .all(|source| !super::archive::is_member(source))
        }
        Operation::Trash(sources) => sources
            .iter()
            .all(|source| !super::archive::is_member(source)),
        _ => false,
    }
}

pub fn enqueue(data: &Path, operation: &Operation, verify: bool) -> io::Result<Job> {
    let (sources, directory, cut) = match operation {
        Operation::Transfer {
            sources,
            directory,
            cut,
        } if supported(operation) => (sources, Some(uri(&directory.canonicalize()?)?), *cut),
        Operation::Trash(sources) if supported(operation) => (sources, None, false),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Operation cannot be journaled",
            ));
        }
    };
    let directory_root = root(data)?;
    // Unique names also let independent windows enqueue without losing updates.
    let reserved = tempfile::Builder::new()
        .prefix("job-")
        .suffix(".reserve")
        .tempfile_in(&directory_root)?;
    let id = reserved
        .path()
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let job = Job {
        version: 1,
        id,
        queued_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos() as u64,
        sources: sources
            .iter()
            .map(|source| {
                let parent = source
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or(Path::new("."));
                uri(&parent
                    .canonicalize()?
                    .join(source.file_name().ok_or_else(invalid)?))
            })
            .collect::<io::Result<_>>()?,
        directory,
        cut,
        verify,
        plan: None,
    };
    // Remove the reservation before publishing; its randomly generated ID stays unique.
    drop(reserved);
    save(data, &job)?;
    Ok(job)
}

pub fn recover(data: &Path) -> io::Result<Vec<Job>> {
    let mut files = fs::read_dir(root(data)?)?.collect::<io::Result<Vec<_>>>()?;
    files.retain(|entry| entry.path().extension().is_some_and(|ext| ext == "json"));
    let mut jobs = files
        .into_iter()
        .map(|file| {
            let job: Job = serde_json::from_reader(File::open(file.path())?)?;
            if journal(data, &job)? != file.path() {
                return Err(invalid());
            }
            validate(&job)?;
            Ok(job)
        })
        .collect::<io::Result<Vec<_>>>()?;
    jobs.sort_by_key(|job| (job.queued_at, job.id.clone()));
    Ok(jobs)
}

fn job_lock(data: &Path, job: &Job) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(journal(data, job)?.with_extension("lock"))?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(io::Error::other(
            "This operation is active in another Virial window",
        ));
    }
    Ok(file)
}

pub fn forget(data: &Path, job: &Job) -> io::Result<()> {
    let _lock = job_lock(data, job)?;
    match File::open(journal(data, job)?) {
        Ok(file) => {
            let saved: Job = serde_json::from_reader(file)?;
            if saved.plan.is_some() {
                return Err(io::Error::other(
                    "Resume this operation and cancel it from its controls",
                ));
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    forget_unlocked(data, job)
}

fn forget_unlocked(data: &Path, job: &Job) -> io::Result<()> {
    let file = journal(data, job)?;
    match fs::remove_file(&file) {
        Ok(()) => {
            let _ = fs::remove_file(file.with_extension("lock"));
            File::open(root(data)?)?.sync_all()
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub fn counts(source: &Path) -> io::Result<(u64, u64)> {
    let metadata = fs::symlink_metadata(source)?;
    let mut files = 1;
    let mut bytes = if metadata.is_file() {
        metadata.len()
    } else {
        0
    };
    if metadata.is_dir() {
        for child in fs::read_dir(source)? {
            let (children, size) = counts(&child?.path())?;
            files += children;
            bytes += size;
        }
    }
    Ok((files, bytes))
}

fn private_directory(parent: &Path, prefix: &str) -> io::Result<String> {
    uri(&tempfile::Builder::new()
        .prefix(prefix)
        .tempdir_in(parent)?
        .keep())
}

fn prepare(data: &Path, job: &Job, progress: &Progress) -> io::Result<Vec<Item>> {
    let mut sources = job
        .sources
        .iter()
        .map(|source| path(source))
        .collect::<io::Result<Vec<_>>>()?;
    sources.sort();
    sources.dedup();
    let folders = sources
        .iter()
        .filter(|source| fs::symlink_metadata(source).is_ok_and(|m| m.is_dir()))
        .cloned()
        .collect::<HashSet<_>>();
    sources.retain(|source| {
        !source
            .ancestors()
            .skip(1)
            .any(|parent| folders.contains(parent))
    });
    let directory = job
        .directory
        .as_ref()
        .map(|directory| path(directory))
        .transpose()?;
    if directory
        .as_ref()
        .is_some_and(|directory| !directory.is_dir())
    {
        return Err(io::Error::other("Destination is not a folder"));
    }
    let journal_root = root(data)?;
    let undo_root = data.join("virial/undo");
    let mut targets = HashSet::new();
    let mut plan = Vec::new();
    for source in sources {
        progress.checkpoint()?;
        if source.starts_with(&journal_root)
            || journal_root.starts_with(&source)
            || source.starts_with(&undo_root)
            || undo_root.starts_with(&source)
        {
            return Err(io::Error::other(
                "Cannot transfer operation journals or undo history",
            ));
        }
        let hash = undo::fingerprint_controlled(&source, progress)?;
        if hash == "-" {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Source no longer exists",
            ));
        }
        let (files, bytes) = counts(&source)?;
        let mut destination = directory
            .as_ref()
            .map(|directory| directory.join(source.file_name().unwrap()));
        if let Some(target) = destination.as_mut() {
            if job.cut && *target == source {
                continue;
            }
            if fs::symlink_metadata(&source)?.is_dir()
                && directory
                    .as_ref()
                    .unwrap()
                    .starts_with(source.canonicalize()?)
            {
                return Err(io::Error::other("Cannot transfer a folder into itself"));
            }
            if targets.contains(target) || fs::symlink_metadata(&*target).is_ok() {
                let existing = undo::fingerprint_controlled(target, progress)?;
                let destination_bytes = counts(target).map(|(_, bytes)| bytes).unwrap_or(0);
                match progress.conflict(Conflict {
                    source: source.clone(),
                    destination: target.clone(),
                    identical: existing == hash
                        || (existing != "-"
                            && bytes == destination_bytes
                            && undo::content_fingerprint(&source, progress)?
                                == undo::content_fingerprint(target, progress)?),
                    source_bytes: bytes,
                    destination_bytes,
                })? {
                    Resolution::Skip => continue,
                    Resolution::KeepBoth => {
                        let stem = target.file_stem().unwrap_or_default().to_os_string();
                        let extension = target.extension().map(|ext| ext.to_os_string());
                        let folder = fs::symlink_metadata(&source)?.is_dir();
                        let mut number = 1;
                        loop {
                            let mut name = if folder {
                                source.file_name().unwrap().to_os_string()
                            } else {
                                stem.clone()
                            };
                            name.push(format!(" ({number})"));
                            if !folder && let Some(extension) = &extension {
                                name.push(".");
                                name.push(extension);
                            }
                            let candidate = target.with_file_name(name);
                            if !targets.contains(&candidate)
                                && undo::fingerprint_controlled(&candidate, progress)? == "-"
                            {
                                *target = candidate;
                                break;
                            }
                            number += 1;
                        }
                    }
                }
            }
            targets.insert(target.clone());
        }
        plan.push(Item {
            source: uri(&source)?,
            destination: destination.as_deref().map(uri).transpose()?,
            staging: None,
            parked: None,
            hash,
            files,
            bytes,
        });
    }
    let allocated = (|| -> io::Result<()> {
        for item in &mut plan {
            progress.checkpoint()?;
            item.staging = directory
                .as_ref()
                .map(|directory| private_directory(directory, ".virial-transfer-"))
                .transpose()?;
            if job.cut {
                item.parked = Some(private_directory(
                    path(&item.source)?.parent().unwrap(),
                    ".virial-move-",
                )?);
            }
        }
        Ok(())
    })();
    if let Err(error) = allocated {
        for item in &plan {
            for location in [&item.staging, &item.parked].into_iter().flatten() {
                discard_private(&path(location)?)?;
            }
        }
        return Err(error);
    }
    Ok(plan)
}

/// Unknown or rotating devices use one worker; nonrotating devices can service
/// independent files concurrently. Bound both CPU usage and outstanding I/O.
fn workers(directory: &Path, files: usize) -> usize {
    let device = fs::metadata(directory).map(|m| m.dev()).unwrap_or(0);
    let sys = PathBuf::from(format!(
        "/sys/dev/block/{}:{}",
        libc::major(device),
        libc::minor(device)
    ));
    let solid = sys.canonicalize().ok().is_some_and(|device| {
        device.ancestors().any(|parent| {
            fs::read_to_string(parent.join("queue/rotational"))
                .is_ok_and(|value| value.trim() == "0")
        })
    });
    if !solid || files < 16 {
        1
    } else {
        std::thread::available_parallelism()
            .map_or(1, |n| n.get())
            .min(4)
            .min(files.div_ceil(16))
    }
}

fn parallel(
    length: usize,
    workers: usize,
    action: impl Fn(usize) -> io::Result<()> + Sync,
) -> io::Result<()> {
    let cursor = AtomicUsize::new(0);
    let stopped = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|scope| -> io::Result<()> {
        let tasks = (0..workers)
            .map(|_| {
                scope.spawn(|| -> io::Result<()> {
                    while !stopped.load(Ordering::Relaxed) {
                        let index = cursor.fetch_add(1, Ordering::Relaxed);
                        if index >= length {
                            break;
                        }
                        if let Err(error) = action(index) {
                            stopped.store(true, Ordering::Relaxed);
                            return Err(error);
                        }
                    }
                    Ok(())
                })
            })
            .collect::<Vec<_>>();
        let mut result = Ok(());
        for task in tasks {
            let worker = task
                .join()
                .map_err(|_| io::Error::other("Transfer worker failed"))
                .and_then(|result| result);
            if result.is_ok() {
                result = worker;
            }
        }
        result
    })
}

fn resume_file(source: &Path, target: &Path, progress: &Progress) -> io::Result<()> {
    progress.checkpoint()?;
    let metadata = fs::symlink_metadata(source)?;
    let mut input = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(source)?;
    let mut output = match OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(target)
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            match OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(target)
            {
                Ok(file) => file,
                Err(error)
                    if error.kind() == io::ErrorKind::PermissionDenied
                        && fs::symlink_metadata(target)?.len() == metadata.len() =>
                {
                    OpenOptions::new()
                        .read(true)
                        .custom_flags(libc::O_NOFOLLOW)
                        .open(target)?
                }
                Err(error) => return Err(error),
            }
        }
        Err(error) => return Err(error),
    };
    let output_metadata = output.metadata()?;
    if !output_metadata.is_file()
        || output_metadata.nlink() != 1
        || output_metadata.len() > metadata.len()
    {
        return Err(io::Error::other("Invalid partial transfer"));
    }
    let offset = output_metadata.len();
    // Verify the entire saved prefix before seeking past it. Never trust length
    // alone: a crash or an external edit can have damaged the partial contents.
    if offset > 0 {
        let mut remaining = offset;
        let mut left = vec![0; 256 * 1024];
        let mut right = vec![0; left.len()];
        while remaining > 0 {
            progress.checkpoint()?;
            let length = remaining.min(left.len() as u64) as usize;
            input.read_exact(&mut left[..length])?;
            output.read_exact(&mut right[..length])?;
            if left[..length] != right[..length] {
                return Err(io::Error::other("Partial transfer failed integrity check"));
            }
            remaining -= length as u64;
        }
        progress.advance(offset);
        progress.transferred(offset, 0);
    }
    input.seek(SeekFrom::Start(offset))?;
    output.seek(SeekFrom::Start(offset))?;
    let mut copied = offset;
    if offset == 0 && metadata.len() >= 128 * 1024 {
        let result = unsafe { libc::ioctl(output.as_raw_fd(), libc::FICLONE, input.as_raw_fd()) };
        if result == 0 {
            copied = metadata.len();
            progress.advance(copied);
            progress.transferred(copied, 0);
        }
    }
    while copied < metadata.len() {
        progress.checkpoint()?;
        let amount = io::copy(
            &mut (&mut input).take((metadata.len() - copied).min(4 * 1024 * 1024)),
            &mut output,
        )?;
        if amount == 0 {
            return Err(io::Error::other("Source changed during transfer"));
        }
        copied += amount;
        progress.advance(amount);
        progress.transferred(amount, 0);
    }
    output.set_permissions(metadata.permissions())?;
    output.sync_all()?;
    progress.advance(1);
    progress.transferred(0, 1);
    Ok(())
}

fn stage_copy(source: &Path, target: &Path, progress: &Progress) -> io::Result<()> {
    let mut pending = vec![(source.to_path_buf(), target.to_path_buf())];
    let mut files = Vec::new();
    let mut directories = Vec::new();
    while let Some((source, target)) = pending.pop() {
        progress.checkpoint()?;
        let metadata = fs::symlink_metadata(&source)?;
        if metadata.is_symlink() {
            let link = fs::read_link(&source)?;
            match symlink(&link, &target) {
                Ok(()) => {}
                Err(error)
                    if error.kind() == io::ErrorKind::AlreadyExists
                        && fs::read_link(&target)? == link => {}
                Err(error) => return Err(error),
            }
            progress.advance(1);
            progress.transferred(0, 1);
        } else if metadata.is_dir() {
            match fs::create_dir(&target) {
                Ok(()) => {}
                Err(error)
                    if error.kind() == io::ErrorKind::AlreadyExists
                        && fs::symlink_metadata(&target)?.is_dir() => {}
                Err(error) => return Err(error),
            }
            fs::set_permissions(&target, fs::Permissions::from_mode(0o700))?;
            let children = fs::read_dir(&source)?.collect::<io::Result<Vec<_>>>()?;
            let names = children
                .iter()
                .map(|entry| entry.file_name())
                .collect::<HashSet<_>>();
            for saved in fs::read_dir(&target)? {
                if !names.contains(&saved?.file_name()) {
                    return Err(io::Error::other("Unexpected contents in partial transfer"));
                }
            }
            for entry in children {
                pending.push((entry.path(), target.join(entry.file_name())));
            }
            directories.push((target, metadata.permissions()));
        } else if metadata.is_file() {
            files.push((source, target));
        } else {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Special files cannot be copied",
            ));
        }
    }
    let worker_count = workers(target.parent().unwrap(), files.len())
        .min(workers(source.parent().unwrap(), files.len()));
    if worker_count == 1 {
        for (source, target) in &files {
            resume_file(source, target, progress)?;
        }
    } else {
        parallel(files.len(), worker_count, |index| {
            let (source, target) = &files[index];
            resume_file(source, target, progress)
        })?;
    }
    for (directory, permissions) in directories.into_iter().rev() {
        progress.checkpoint()?;
        fs::set_permissions(&directory, permissions)?;
        File::open(directory)?.sync_all()?;
        progress.advance(1);
        progress.transferred(0, 1);
    }
    Ok(())
}

fn sync_parent(path: &Path) -> io::Result<()> {
    File::open(path.parent().ok_or_else(invalid)?)?.sync_all()
}

fn transfer(item: &Item, cut: bool, verify: bool, progress: &Progress) -> io::Result<()> {
    progress.checkpoint()?;
    let source = path(&item.source)?;
    let destination = path(item.destination.as_ref().ok_or_else(invalid)?)?;
    let current = undo::fingerprint_controlled(&destination, progress)?;
    if current == "-" {
        if undo::fingerprint_controlled(&source, progress)? != item.hash {
            return Err(io::Error::other("Source changed since transfer was queued"));
        }
        if cut {
            match operations::rename(&source, &destination) {
                Ok(()) => {
                    sync_parent(&source)?;
                    sync_parent(&destination)?;
                    progress.advance(item.bytes + item.files);
                    progress.transferred(item.bytes, item.files);
                    return Ok(());
                }
                Err(error) if error.raw_os_error() == Some(libc::EXDEV) => {}
                Err(error) => return Err(error),
            }
        }
        let staged = path(item.staging.as_ref().ok_or_else(invalid)?)?.join("contents");
        stage_copy(&source, &staged, progress)?;
        if undo::fingerprint_controlled(&source, progress)? != item.hash
            || ((cut || verify || progress.verification())
                && undo::fingerprint_controlled(&staged, progress)? != item.hash)
        {
            return Err(io::Error::other(
                "Transfer failed integrity check; original retained",
            ));
        }
        progress.checkpoint()?;
        operations::rename(&staged, &destination)?;
        sync_parent(&destination)?;
    } else {
        if current != item.hash {
            return Err(io::Error::other("Destination changed after interruption"));
        }
        progress.advance(item.bytes + item.files);
        progress.transferred(item.bytes, item.files);
    }
    if cut && undo::fingerprint_controlled(&source, progress)? != "-" {
        if undo::fingerprint_controlled(&source, progress)? != item.hash {
            return Err(io::Error::other("Source changed; original retained"));
        }
        let parked = path(item.parked.as_ref().ok_or_else(invalid)?)?.join("contents");
        // Atomic source removal keeps crash recovery away from partially deleted trees.
        operations::rename(&source, &parked)?;
        sync_parent(&source)?;
        sync_parent(&parked)?;
    }
    Ok(())
}

fn discard_private(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_dir() {
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(metadata.permissions().mode() | 0o700),
        )?;
        for entry in fs::read_dir(path)? {
            discard_private(&entry?.path())?;
        }
        fs::remove_dir(path)
    } else {
        fs::remove_file(path)
    }
}

fn validate(job: &Job) -> io::Result<()> {
    if job.version != 1 {
        return Err(invalid());
    }
    if let Some(plan) = &job.plan {
        let directory = job.directory.as_deref().map(path).transpose()?;
        let mut targets = HashSet::new();
        for item in plan {
            let source = path(&item.source)?;
            if !job.sources.contains(&item.source)
                || item.hash.len() != 64
                || !item.hash.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err(invalid());
            }
            if let Some(destination) = &item.destination {
                let destination = path(destination)?;
                if destination.parent() != directory.as_deref() || !targets.insert(destination) {
                    return Err(invalid());
                }
            } else if directory.is_some() {
                return Err(invalid());
            }
            for (location, parent, prefix) in [
                (&item.staging, directory.as_deref(), ".virial-transfer-"),
                (&item.parked, source.parent(), ".virial-move-"),
            ] {
                if let Some(location) = location {
                    let location = path(location)?;
                    if location.parent() != parent
                        || !location.file_name().is_some_and(|name| {
                            name.as_encoded_bytes().starts_with(prefix.as_bytes())
                        })
                    {
                        return Err(invalid());
                    }
                    if let Ok(metadata) = fs::symlink_metadata(location)
                        && (!metadata.is_dir() || metadata.uid() != unsafe { libc::geteuid() })
                    {
                        return Err(invalid());
                    }
                }
            }
        }
    }
    Ok(())
}

fn cleanup(job: &Job) -> io::Result<()> {
    validate(job)?;
    for item in job.plan.iter().flatten() {
        for location in [&item.staging, &item.parked].into_iter().flatten() {
            let private = path(location)?;
            if fs::symlink_metadata(&private).is_ok() {
                if item.parked.as_ref() == Some(location)
                    && fs::symlink_metadata(private.join("contents")).is_ok()
                    && undo::fingerprint(&private.join("contents"))? != item.hash
                {
                    return Err(io::Error::other(format!(
                        "Original changed; retained at {}",
                        private.display()
                    )));
                }
                discard_private(&private)?;
                sync_parent(&private)?;
            }
        }
    }
    Ok(())
}

pub fn operation(job: &Job) -> io::Result<Operation> {
    let sources = job
        .sources
        .iter()
        .map(|source| path(source))
        .collect::<io::Result<Vec<_>>>()?;
    Ok(if let Some(directory) = &job.directory {
        Operation::Transfer {
            sources,
            directory: path(directory)?,
            cut: job.cut,
        }
    } else {
        Operation::Trash(sources)
    })
}

pub fn label(job: &Job) -> &'static str {
    if job.directory.is_none() {
        "Move to Trash…"
    } else if job.cut {
        "Moving…"
    } else {
        "Copying…"
    }
}

pub fn execute(data: &Path, mut job: Job, progress: &Progress) -> io::Result<()> {
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(root(data)?.join("lock"))?;
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(io::Error::other(
            "Another Virial window is running the operation queue",
        ));
    }
    let _job_lock = job_lock(data, &job)?;
    // A different window may already have completed or cancelled this job.
    File::open(journal(data, &job)?)?;
    validate(&job)?;
    progress.configure_verification(job.verify);
    if job.plan.is_none() {
        let plan = match prepare(data, &job, progress) {
            Ok(plan) => plan,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                forget_unlocked(data, &job)?;
                return Err(error);
            }
            Err(error) => return Err(error),
        };
        job.plan = Some(plan);
        save(data, &job)?;
    }
    let plan = job.plan.as_ref().unwrap();
    let files = plan.iter().map(|item| item.files).sum();
    let bytes = plan.iter().map(|item| item.bytes).sum();
    progress.totals(files, bytes);
    let mut expected = Vec::new();
    for item in plan {
        if job.cut || job.directory.is_none() {
            expected.push((path(&item.source)?, "-".into()));
        }
        if let Some(destination) = &item.destination {
            expected.push((path(destination)?, item.hash.clone()));
        }
    }
    let summary = if job.directory.is_none() {
        format!("Trash\t{files}")
    } else {
        format!("Transfer\t{files}")
    };
    let result = undo::durable(data, &job.id, expected, &summary, progress, || {
        progress.begin(
            if job.cut {
                Phase::Moving
            } else {
                Phase::Copying
            },
            Some(bytes + files),
        );
        let verify = progress.verification();
        let mut saved = job.clone();
        saved.verify = verify;
        save(data, &saved)?;
        // Only flat file batches use outer workers, avoiding nested worker pools.
        if let Some(directory) = &job.directory {
            let directory = path(directory)?;
            let flat = plan.iter().all(|item| {
                path(&item.source).ok().is_some_and(|source| {
                    fs::symlink_metadata(source).is_ok_and(|metadata| metadata.is_file())
                })
            });
            let count = if flat {
                plan.iter().try_fold(
                    workers(&directory, plan.len()),
                    |count, item| -> io::Result<usize> {
                        Ok(count.min(workers(path(&item.source)?.parent().unwrap(), plan.len())))
                    },
                )?
            } else {
                1
            };
            if count > 1 {
                return parallel(plan.len(), count, |index| {
                    transfer(&plan[index], job.cut, false, progress)
                });
            }
        }
        for item in plan {
            progress.checkpoint()?;
            if job.directory.is_some() {
                transfer(item, job.cut, false, progress)?;
            } else {
                let source = path(&item.source)?;
                if undo::fingerprint_controlled(&source, progress)? != "-" {
                    if undo::fingerprint_controlled(&source, progress)? != item.hash {
                        return Err(io::Error::other("Source changed before moving to Trash"));
                    }
                    let output = Command::new("gio")
                        .arg("trash")
                        .arg("--")
                        .arg(&source)
                        .output()?;
                    if !output.status.success() {
                        return Err(io::Error::other(
                            String::from_utf8_lossy(&output.stderr).into_owned(),
                        ));
                    }
                    sync_parent(&source)?;
                }
                progress.advance(item.files + item.bytes);
                progress.transferred(item.bytes, item.files);
            }
        }
        Ok(())
    });
    if result
        .as_ref()
        .is_err_and(|error| error.kind() == io::ErrorKind::Interrupted)
        && progress.snapshot().phase != Phase::Finishing
    {
        cleanup(&job)?;
        forget_unlocked(data, &job)?;
    }
    // Leave recovery errors and their backups/journal available for retry.
    if undo::finalized(data, &job.id)? {
        cleanup(&job)?;
        forget_unlocked(data, &job)?;
        undo::discard_noop(data, &job.id)?;
    }
    result
}

#[cfg(test)]
#[path = "../../tests/infrastructure/queue.rs"]
mod tests;
