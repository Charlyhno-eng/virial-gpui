//! Shared name catalog: filesystem work is independent of text input.
use super::{RESULT_LIMIT, SearchResults, score};
use crate::domain::models::Entry;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BinaryHeap, HashMap, HashSet, VecDeque},
    ffi::{CString, OsString},
    fs::{self, File},
    io::{self, BufRead, BufReader, BufWriter, Write},
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::{
            ffi::{OsStrExt, OsStringExt},
            fs::MetadataExt,
        },
    },
    path::{Path, PathBuf},
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Stamp(u64, u64, i64, i64, i64, i64);

impl Stamp {
    fn read(metadata: &fs::Metadata) -> Self {
        Self(
            metadata.dev(),
            metadata.ino(),
            metadata.mtime(),
            metadata.mtime_nsec(),
            metadata.ctime(),
            metadata.ctime_nsec(),
        )
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
struct StoredItem {
    name: Vec<u8>,
    kind: u8, // 0: file, 1: directory, 2: symlink
}

#[derive(Serialize, Deserialize)]
struct StoredDirectory {
    path: Vec<u8>,
    stamp: Stamp,
    items: Vec<StoredItem>,
}

struct Item {
    name: String,
    name_key: String,
    path: PathBuf,
    path_key: String,
    letters: u128,
    kind: u8,
    hidden: bool,
}

struct Directory {
    stored: StoredDirectory,
    items: Vec<Item>,
}

// Collisions only admit extra candidates; they never exclude a valid Unicode match.
fn letter_mask(text: &str) -> u128 {
    text.chars()
        .fold(0, |mask, ch| mask | (1 << ((ch as u32) % 128)))
}

impl Directory {
    fn new(stored: StoredDirectory, roots: &[PathBuf]) -> Self {
        let path = PathBuf::from(OsString::from_vec(stored.path.clone()));
        let hidden_parent = roots
            .iter()
            .find_map(|root| path.strip_prefix(root).ok())
            .unwrap_or(&path)
            .components()
            .any(|part| part.as_os_str().as_bytes().starts_with(b"."));
        let mut items: Vec<_> = stored
            .items
            .iter()
            .map(|item| {
                let raw_name = OsString::from_vec(item.name.clone());
                let name = raw_name.to_string_lossy().into_owned();
                let path = path.join(raw_name);
                let path_key = path.to_string_lossy().to_lowercase();
                Item {
                    name_key: name.to_lowercase(),
                    hidden: hidden_parent || name.starts_with('.'),
                    name,
                    letters: letter_mask(&path_key),
                    path,
                    path_key,
                    kind: item.kind,
                }
            })
            .collect();
        items.sort_unstable_by(|a, b| (&a.name_key, &a.path).cmp(&(&b.name_key, &b.path)));
        Self { stored, items }
    }
}

#[derive(Default)]
struct Catalog {
    directories: BTreeMap<PathBuf, Arc<Directory>>,
    revision: u64,
    finished: bool,
    skipped: usize,
}

#[derive(Clone, Default)]
pub(crate) struct SearchHandle(Arc<RwLock<Catalog>>);

pub(crate) struct SearchIndex {
    handle: SearchHandle,
    stopped: Arc<AtomicBool>,
}

impl Drop for SearchIndex {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
    }
}

impl SearchIndex {
    pub(crate) fn start(roots: Vec<PathBuf>, cache: PathBuf) -> Self {
        let handle = SearchHandle::default();
        let stopped = Arc::new(AtomicBool::new(false));
        let worker_handle = handle.clone();
        let worker_stopped = stopped.clone();
        // A dedicated thread keeps slow mounts and inventory I/O off GPUI's executor.
        if thread::Builder::new()
            .name("virial-search-index".into())
            .spawn(move || {
                let mut inventory = Inventory::new(worker_handle, roots, Some(cache));
                inventory.run(&worker_stopped);
            })
            .is_err()
        {
            let mut catalog = handle.0.write().unwrap();
            catalog.finished = true;
            catalog.skipped = 1;
        }
        Self { handle, stopped }
    }

    pub(crate) fn handle(&self) -> SearchHandle {
        self.handle.clone()
    }
}

// Heap entries borrow immutable snapshots: only the final 100 results allocate Entries.
#[derive(Eq, PartialEq)]
struct Match<'a> {
    rank: usize,
    item: &'a Item,
}

impl PartialEq for Item {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
    }
}
impl Eq for Item {}
impl Ord for Match<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.rank, &self.item.path_key, &self.item.path).cmp(&(
            other.rank,
            &other.item.path_key,
            &other.item.path,
        ))
    }
}
impl PartialOrd for Match<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn retain_match<'a>(heap: &mut BinaryHeap<Match<'a>>, rank: usize, item: &'a Item) {
    let candidate = Match { rank, item };
    if heap.len() < RESULT_LIMIT {
        heap.push(candidate);
    } else if heap.peek().is_some_and(|worst| candidate < *worst) {
        *heap.peek_mut().unwrap() = candidate;
    }
}

impl SearchHandle {
    pub(crate) fn revision(&self) -> u64 {
        self.0.read().unwrap().revision
    }

    pub(crate) fn query(
        &self,
        query: &str,
        hidden: bool,
        cancelled: &AtomicBool,
    ) -> Option<SearchResults> {
        let terms: Vec<_> = query.split_whitespace().map(str::to_lowercase).collect();
        if terms.is_empty() || cancelled.load(Ordering::Relaxed) {
            return None;
        }
        // Hold the lock only to copy snapshot references, never during matching or I/O.
        let (directories, skipped, finished) = {
            let catalog = self.0.read().unwrap();
            (
                catalog.directories.values().cloned().collect::<Vec<_>>(),
                catalog.skipped,
                catalog.finished,
            )
        };
        let mut best = BinaryHeap::with_capacity(RESULT_LIMIT + 1);
        if terms.len() == 1 {
            let term = &terms[0];
            for directory in &directories {
                if cancelled.load(Ordering::Relaxed) {
                    return None;
                }
                // Names are sorted once at inventory time. Common prefix searches can
                // finish without scoring every path, even with millions of files.
                let start = directory
                    .items
                    .partition_point(|item| item.name_key < *term);
                let mut retained = 0;
                for (index, item) in directory.items[start..]
                    .iter()
                    .take_while(|item| item.name_key.starts_with(term))
                    .enumerate()
                {
                    if index % 256 == 0 && cancelled.load(Ordering::Relaxed) {
                        return None;
                    }
                    if hidden || !item.hidden {
                        retain_match(&mut best, usize::from(item.name_key != *term), item);
                        retained += 1;
                        // Within one parent, this order is also the ranking order.
                        // No later prefix can enter the global top 100.
                        if retained == RESULT_LIMIT {
                            break;
                        }
                    }
                }
            }
        }
        if best.len() < RESULT_LIMIT {
            best.clear();
            let required = terms.iter().fold(0, |mask, term| mask | letter_mask(term));
            for directory in &directories {
                for (index, item) in directory.items.iter().enumerate() {
                    if index % 256 == 0 && cancelled.load(Ordering::Relaxed) {
                        return None;
                    }
                    if (!hidden && item.hidden) || item.letters & required != required {
                        continue;
                    }
                    if let Some(rank) = score(&item.name_key, &item.path_key, &terms) {
                        retain_match(&mut best, rank, item);
                    }
                }
            }
        }
        let mut entries = Vec::with_capacity(best.len());
        for result in best.into_sorted_vec() {
            if cancelled.load(Ordering::Relaxed) {
                return None;
            }
            let item = result.item;
            entries.push(Entry {
                path: item.path.clone(),
                name: item.name.clone(),
                bytes: None,
                // Resolve only selected symlinks, never every file in the catalog.
                directory: item.kind == 1 || (item.kind == 2 && item.path.is_dir()),
            });
        }
        Some(SearchResults {
            entries,
            skipped,
            finished,
        })
    }
}

struct Watches {
    fd: Option<OwnedFd>,
    paths: HashMap<i32, PathBuf>,
}

impl Watches {
    fn new() -> Self {
        // SAFETY: inotify_init1 takes only flags and returns a new owned descriptor.
        let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        Self {
            fd: (fd >= 0).then(|| unsafe { OwnedFd::from_raw_fd(fd) }),
            paths: HashMap::new(),
        }
    }

    fn add(&mut self, path: &Path) {
        let Some(fd) = &self.fd else {
            return;
        };
        let Ok(path_c) = CString::new(path.as_os_str().as_bytes()) else {
            return;
        };
        let mask = libc::IN_CREATE
            | libc::IN_DELETE
            | libc::IN_MOVED_FROM
            | libc::IN_MOVED_TO
            | libc::IN_DELETE_SELF
            | libc::IN_MOVE_SELF
            | libc::IN_UNMOUNT
            | libc::IN_ONLYDIR;
        // SAFETY: the descriptor is owned and path_c is NUL-terminated.
        let wd = unsafe { libc::inotify_add_watch(fd.as_raw_fd(), path_c.as_ptr(), mask) };
        if wd >= 0 {
            self.paths.insert(wd, path.to_path_buf());
        }
    }

    fn remove_tree(&mut self, root: &Path) {
        self.paths.retain(|wd, path| {
            if !path.starts_with(root) {
                return true;
            }
            if let Some(fd) = &self.fd {
                // SAFETY: the descriptor belongs to this watch set.
                unsafe {
                    libc::inotify_rm_watch(fd.as_raw_fd(), *wd);
                }
            }
            false
        });
    }

    fn changes(&mut self) -> (HashSet<PathBuf>, bool) {
        let mut changed = HashSet::new();
        let mut overflow = false;
        let Some(fd) = &self.fd else {
            return (changed, overflow);
        };
        let mut buffer = [0u8; 64 * 1024];
        loop {
            // SAFETY: the buffer is writable for its full length and fd is valid.
            let count =
                unsafe { libc::read(fd.as_raw_fd(), buffer.as_mut_ptr().cast(), buffer.len()) };
            if count <= 0 {
                break;
            }
            let mut offset = 0;
            while offset + std::mem::size_of::<libc::inotify_event>() <= count as usize {
                // inotify records are packed in a byte buffer, which need not be aligned.
                let event = unsafe {
                    std::ptr::read_unaligned(
                        buffer.as_ptr().add(offset).cast::<libc::inotify_event>(),
                    )
                };
                offset += std::mem::size_of::<libc::inotify_event>() + event.len as usize;
                overflow |= event.mask & libc::IN_Q_OVERFLOW != 0;
                if let Some(path) = self.paths.get(&event.wd) {
                    changed.insert(path.clone());
                }
                if event.mask & libc::IN_IGNORED != 0 {
                    self.paths.remove(&event.wd);
                }
            }
        }
        (changed, overflow)
    }
}

struct Inventory {
    handle: SearchHandle,
    roots: Vec<PathBuf>,
    cache: Option<PathBuf>,
    watches: Watches,
    pending: VecDeque<PathBuf>,
    visited: HashSet<(u64, u64)>,
    seen: HashSet<PathBuf>,
    full_scan: bool,
    force_scan: bool,
    invalidated: HashSet<PathBuf>,
    rescan: bool,
    dirty: bool,
}

impl Inventory {
    fn new(handle: SearchHandle, roots: Vec<PathBuf>, cache: Option<PathBuf>) -> Self {
        Self {
            handle,
            roots,
            cache,
            watches: Watches::new(),
            pending: VecDeque::new(),
            visited: HashSet::new(),
            seen: HashSet::new(),
            full_scan: false,
            force_scan: false,
            invalidated: HashSet::new(),
            rescan: false,
            dirty: false,
        }
    }

    fn excluded(&self, path: &Path) -> bool {
        ["/proc", "/sys", "/dev"]
            .iter()
            .any(|root| path.starts_with(root))
            || self
                .cache
                .as_ref()
                .and_then(|cache| cache.parent())
                .is_some_and(|cache| path.starts_with(cache))
    }

    fn begin_scan(&mut self, force: bool) {
        self.full_scan = true;
        self.force_scan = force;
        self.pending = self.roots.iter().cloned().collect();
        self.visited.clear();
        self.seen.clear();
        let mut catalog = self.handle.0.write().unwrap();
        catalog.finished = false;
        catalog.skipped = 0;
        catalog.revision += 1;
    }

    fn remove_tree(&mut self, root: &Path) {
        let mut catalog = self.handle.0.write().unwrap();
        let before = catalog.directories.len();
        catalog
            .directories
            .retain(|path, _| !path.starts_with(root));
        if before != catalog.directories.len() {
            catalog.revision += 1;
            self.dirty = true;
        }
        drop(catalog);
        self.watches.remove_tree(root);
    }

    fn scan_directory(&mut self, path: PathBuf, stopped: &AtomicBool) {
        if self.excluded(&path) {
            return;
        }
        let metadata = match fs::metadata(&path) {
            Ok(metadata) if metadata.is_dir() => metadata,
            result => {
                self.remove_tree(&path);
                // Removal or replacement is a normal watch event, not an unreadable location.
                if result
                    .as_ref()
                    .err()
                    .is_some_and(|error| error.kind() != io::ErrorKind::NotFound)
                    || self.roots.contains(&path)
                {
                    let mut catalog = self.handle.0.write().unwrap();
                    catalog.skipped += 1;
                    catalog.revision += 1;
                }
                return;
            }
        };
        if !self.visited.insert((metadata.dev(), metadata.ino())) {
            return;
        }
        self.seen.insert(path.clone());
        // Install before reading so changes during enumeration are caught.
        self.watches.add(&path);
        let previous = self
            .handle
            .0
            .read()
            .unwrap()
            .directories
            .get(&path)
            .cloned();
        let stamp = Stamp::read(&metadata);
        let invalidated = self.invalidated.remove(&path);
        let force = self.force_scan || invalidated;
        let directory = if let Some(previous) = previous
            .as_ref()
            .filter(|previous| !force && previous.stored.stamp == stamp)
        {
            previous.clone()
        } else {
            let items = match fs::read_dir(&path) {
                Ok(items) => items,
                Err(_) => {
                    self.remove_tree(&path);
                    let mut catalog = self.handle.0.write().unwrap();
                    catalog.skipped += 1;
                    catalog.revision += 1;
                    return;
                }
            };
            let mut stored = Vec::new();
            let mut skipped = 0;
            for item in items {
                if stopped.load(Ordering::Relaxed) {
                    return;
                }
                let Ok(item) = item else {
                    skipped += 1;
                    continue;
                };
                let Ok(kind) = item.file_type() else {
                    skipped += 1;
                    continue;
                };
                stored.push(StoredItem {
                    name: item.file_name().into_vec(),
                    kind: if kind.is_dir() {
                        1
                    } else if kind.is_symlink() {
                        2
                    } else {
                        0
                    },
                });
            }
            stored.sort_unstable_by(|a, b| a.name.cmp(&b.name));
            if let Some(previous) = previous.as_ref().filter(|previous| {
                previous.stored.stamp == stamp && previous.stored.items == stored
            }) {
                previous.clone()
            } else {
                let directory = Arc::new(Directory::new(
                    StoredDirectory {
                        path: path.as_os_str().as_bytes().to_vec(),
                        stamp,
                        items: stored,
                    },
                    &self.roots,
                ));
                let mut catalog = self.handle.0.write().unwrap();
                catalog.directories.insert(path.clone(), directory.clone());
                catalog.skipped += skipped;
                catalog.revision += 1;
                self.dirty = true;
                directory
            }
        };
        if let Some(previous) = &previous {
            let children: HashSet<_> = directory
                .items
                .iter()
                .filter(|item| item.kind == 1)
                .map(|item| &item.path)
                .collect();
            for child in previous
                .items
                .iter()
                .filter(|item| item.kind == 1 && !children.contains(&item.path))
            {
                self.remove_tree(&child.path);
            }
        }
        let known = self.handle.0.read().unwrap();
        let children: Vec<_> = directory
            .items
            .iter()
            .filter(|item| item.kind == 1)
            .filter(|item| self.full_scan || !known.directories.contains_key(&item.path))
            .map(|item| item.path.clone())
            .collect();
        drop(known);
        // Complete the first (home) root before starting lower-priority roots.
        if self.full_scan {
            let next_root = self
                .pending
                .iter()
                .position(|path| self.roots.contains(path))
                .unwrap_or(self.pending.len());
            for (offset, child) in children.into_iter().enumerate() {
                self.pending.insert(next_root + offset, child);
            }
        } else {
            self.pending.extend(children);
        }
    }

    fn finish_scan(&mut self) {
        if !self.full_scan {
            return;
        }
        let mut catalog = self.handle.0.write().unwrap();
        let before = catalog.directories.len();
        catalog
            .directories
            .retain(|path, _| self.seen.contains(path));
        self.dirty |= before != catalog.directories.len();
        catalog.finished = true;
        catalog.revision += 1;
        self.full_scan = false;
        self.force_scan = false;
    }

    fn header(&self) -> (u32, Vec<Vec<u8>>) {
        (
            1,
            self.roots
                .iter()
                .map(|path| path.as_os_str().as_bytes().to_vec())
                .collect(),
        )
    }

    fn load(&mut self, stopped: &AtomicBool) -> io::Result<()> {
        let Some(path) = &self.cache else {
            return Ok(());
        };
        let mut lines = BufReader::new(File::open(path)?).lines();
        let header = lines
            .next()
            .transpose()?
            .ok_or(io::ErrorKind::InvalidData)?;
        let header: (u32, Vec<Vec<u8>>) = serde_json::from_str(&header)?;
        if header != self.header() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        for line in lines {
            if stopped.load(Ordering::Relaxed) {
                break;
            }
            let stored: StoredDirectory = serde_json::from_str(&line?)?;
            let path = PathBuf::from(OsString::from_vec(stored.path.clone()));
            if self.excluded(&path) || !self.roots.iter().any(|root| path.starts_with(root)) {
                continue;
            }
            let directory = Arc::new(Directory::new(stored, &self.roots));
            let mut catalog = self.handle.0.write().unwrap();
            catalog.directories.insert(path, directory);
            catalog.revision += 1;
        }
        Ok(())
    }

    fn save(&mut self) -> io::Result<()> {
        let Some(path) = &self.cache else {
            return Ok(());
        };
        let parent = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
        fs::create_dir_all(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        let directories: Vec<_> = self
            .handle
            .0
            .read()
            .unwrap()
            .directories
            .values()
            .cloned()
            .collect();
        {
            let mut writer = BufWriter::new(file.as_file_mut());
            serde_json::to_writer(&mut writer, &self.header())?;
            writer.write_all(b"\n")?;
            for directory in directories {
                serde_json::to_writer(&mut writer, &directory.stored)?;
                writer.write_all(b"\n")?;
            }
            writer.flush()?;
        }
        file.persist(path).map_err(|error| error.error)?;
        self.dirty = false;
        Ok(())
    }

    fn queue_changes(&mut self, changed: HashSet<PathBuf>, overflow: bool) {
        if !self.full_scan && self.pending.is_empty() {
            self.visited.clear();
        }
        if overflow {
            if self.full_scan {
                self.rescan = true;
            } else {
                self.begin_scan(true);
            }
        }
        for path in changed {
            self.invalidated.insert(path.clone());
            if let Ok(metadata) = fs::metadata(&path) {
                self.visited.remove(&(metadata.dev(), metadata.ino()));
            }
            if !self.pending.contains(&path) {
                self.pending.push_back(path);
            }
        }
    }

    fn run(&mut self, stopped: &AtomicBool) {
        // Cache failures are recoverable: always reconcile against the real filesystem.
        let _ = self.load(stopped);
        self.begin_scan(true);
        let mut reconciled = Instant::now();
        let mut checkpoint = Instant::now();
        let mut events = Instant::now();
        while !stopped.load(Ordering::Relaxed) {
            if events.elapsed() >= Duration::from_millis(50) {
                let (changed, overflow) = self.watches.changes();
                self.queue_changes(changed, overflow);
                events = Instant::now();
            }
            if let Some(path) = self.pending.pop_front() {
                self.scan_directory(path, stopped);
                // Keep a partial catalog across restarts even during a long inventory.
                if self.dirty && checkpoint.elapsed() >= Duration::from_secs(30) {
                    let _ = self.save();
                    checkpoint = Instant::now();
                }
                continue;
            }
            if self.full_scan {
                self.finish_scan();
                reconciled = Instant::now();
                if self.dirty {
                    let _ = self.save();
                    checkpoint = Instant::now();
                }
                if self.rescan {
                    self.rescan = false;
                    self.begin_scan(true);
                    continue;
                }
            }
            if self.dirty && checkpoint.elapsed() >= Duration::from_secs(5) {
                let _ = self.save();
                checkpoint = Instant::now();
            }
            // Reconciliation covers mount changes, watch limits, and filesystems
            // (including network mounts) which do not provide reliable inotify events.
            if reconciled.elapsed() >= Duration::from_secs(60) {
                self.begin_scan(true);
            }
            thread::park_timeout(Duration::from_millis(50));
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/infrastructure/search_index.rs"]
mod tests;
