//! Shared name catalog: filesystem work is independent of text input.
use super::{RESULT_LIMIT, SearchResults, score};
use crate::domain::models::Entry;
use serde::{Deserialize, Serialize};
// Used only by the inotify watcher below, which is Linux-only; keep the
// imports scoped the same way so other targets do not warn about them.
#[cfg(target_os = "linux")]
use std::collections::HashMap;
#[cfg(target_os = "linux")]
use std::ffi::CString;
#[cfg(target_os = "linux")]
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
#[cfg(unix)]
use std::os::unix::{
    ffi::{OsStrExt, OsStringExt},
    fs::MetadataExt,
};
use std::{
    collections::{BTreeMap, BinaryHeap, HashSet, VecDeque},
    ffi::{OsStr, OsString},
    fs::{self, File},
    io::{self, BufRead, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

/// What the walk keys its "already visited folders" set on: inode identity
/// where it exists, resolved paths where it does not (Windows).
#[cfg(unix)]
type VisitIdentity = (u64, u64);
#[cfg(windows)]
type VisitIdentity = PathBuf;

#[cfg(windows)]
fn windows_identity(path: &Path) -> PathBuf {
    // Directories all report len 0, so a size-based identity would collapse
    // the whole walk onto the first folder visited and silently skip the rest;
    // resolved paths are stable, and the walk never follows links.
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(unix)]
fn os_bytes(value: &OsStr) -> &[u8] {
    value.as_bytes()
}

#[cfg(windows)]
fn os_bytes(value: &OsStr) -> Vec<u8> {
    value.to_string_lossy().as_bytes().to_vec()
}

#[cfg(unix)]
fn bytes_to_os(value: &[u8]) -> OsString {
    OsString::from_vec(value.to_vec())
}

#[cfg(windows)]
fn bytes_to_os(value: &[u8]) -> OsString {
    OsString::from(String::from_utf8_lossy(value).into_owned())
}

#[cfg(unix)]
fn path_bytes(value: &Path) -> Vec<u8> {
    value.as_os_str().as_bytes().to_vec()
}

#[cfg(windows)]
fn path_bytes(value: &Path) -> Vec<u8> {
    value.to_string_lossy().as_bytes().to_vec()
}

#[cfg(unix)]
fn os_name_bytes(value: &std::ffi::OsStr) -> Vec<u8> {
    value.as_bytes().to_vec()
}

#[cfg(windows)]
fn os_name_bytes(value: &std::ffi::OsStr) -> Vec<u8> {
    value.to_string_lossy().as_bytes().to_vec()
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Stamp(u64, u64, i64, i64, i64, i64);

impl Stamp {
    #[cfg(unix)]
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

    #[cfg(windows)]
    fn read(metadata: &fs::Metadata) -> Self {
        let secs = |time: std::io::Result<std::time::SystemTime>| {
            time.ok()
                .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0)
        };
        Self(
            metadata.len(),
            0,
            secs(metadata.modified()),
            0,
            secs(metadata.created()),
            0,
        )
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
struct StoredItem {
    name: Box<[u8]>,
    kind: u8, // 0: file, 1: directory, 2: symlink
}

#[derive(Serialize, Deserialize)]
struct StoredDirectory {
    path: Vec<u8>,
    stamp: Stamp,
    items: Box<[StoredItem]>,
}

// Store only the matching key and an index into the cached raw names. Parent
// paths are shared by all items; full paths are allocated only for final results.
struct Item {
    name_key: Box<str>,
    // Vec indices cannot use the highest bit: Rust allocations fit in isize.
    // Share that bit with the hidden flag, and avoid u128's 16-byte alignment.
    stored_index: usize,
    letters: [u64; 2],
}

impl Item {
    const HIDDEN_BIT: usize = 1 << (usize::BITS - 1);

    fn raw_index(&self) -> usize {
        self.stored_index & !Self::HIDDEN_BIT
    }

    fn hidden(&self) -> bool {
        self.stored_index & Self::HIDDEN_BIT != 0
    }
}

struct Directory {
    stored: StoredDirectory,
    path: PathBuf,
    path_key: String,
    items: Box<[Item]>,
}

// Collisions only admit extra candidates; they never exclude a valid Unicode match.
fn letter_mask(text: &str) -> u128 {
    text.chars()
        .fold(0, |mask, ch| mask | (1 << ((ch as u32) % 128)))
}

impl Directory {
    fn new(stored: StoredDirectory, roots: &[PathBuf]) -> Self {
        let path = PathBuf::from(bytes_to_os(&stored.path));
        let hidden_parent = roots
            .iter()
            .find_map(|root| path.strip_prefix(root).ok())
            .unwrap_or(&path)
            .components()
            .any(|part| os_bytes(part.as_os_str()).starts_with(b"."));
        let mut path_key = path.to_string_lossy().to_lowercase();
        if !path_key.ends_with('/') {
            path_key.push('/');
        }
        let parent_letters = letter_mask(&path_key);
        let mut items: Vec<_> = stored
            .items
            .iter()
            .enumerate()
            .map(|(stored_index, item)| {
                let name = bytes_to_os(&item.name)
                    .as_os_str()
                    .to_string_lossy()
                    .into_owned();
                let name_key = name.to_lowercase().into_boxed_str();
                let letters = parent_letters | letter_mask(&name_key);
                let hidden = hidden_parent || name.starts_with('.');
                Item {
                    letters: [letters as u64, (letters >> 64) as u64],
                    name_key,
                    stored_index: stored_index | if hidden { Item::HIDDEN_BIT } else { 0 },
                }
            })
            .collect();
        items.sort_unstable_by(|a, b| {
            (&a.name_key, &stored.items[a.raw_index()].name)
                .cmp(&(&b.name_key, &stored.items[b.raw_index()].name))
        });
        Self {
            stored,
            path,
            path_key,
            items: items.into_boxed_slice(),
        }
    }

    fn raw_item(&self, item: &Item) -> &StoredItem {
        &self.stored.items[item.raw_index()]
    }

    fn item_path(&self, item: &Item) -> PathBuf {
        self.path
            .join(bytes_to_os(&self.raw_item(item).name).as_os_str())
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
struct Match<'a> {
    rank: usize,
    item: &'a Item,
    directory: &'a Directory,
}

// Compare two parent/name pairs using slice comparisons, so shared path prefixes
// use memcmp rather than walking chained byte iterators for every candidate.
fn joined_key_cmp<'a>(
    mut left: &'a [u8],
    mut left_name: &'a [u8],
    mut right: &'a [u8],
    mut right_name: &'a [u8],
) -> std::cmp::Ordering {
    loop {
        if left.is_empty() {
            left = left_name;
            left_name = &[];
        }
        if right.is_empty() {
            right = right_name;
            right_name = &[];
        }
        if left.is_empty() || right.is_empty() {
            return left.len().cmp(&right.len());
        }
        let count = left.len().min(right.len());
        let order = left[..count].cmp(&right[..count]);
        if !order.is_eq() {
            return order;
        }
        left = &left[count..];
        right = &right[count..];
    }
}

impl PartialEq for Match<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}
impl Eq for Match<'_> {}
impl Ord for Match<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Compare the joined paths without allocating strings for heap candidates.
        let raw = |candidate: &Self| {
            candidate
                .directory
                .stored
                .path
                .iter()
                .copied()
                .chain((!candidate.directory.stored.path.ends_with(b"/")).then_some(b'/'))
                .chain(
                    candidate
                        .directory
                        .raw_item(candidate.item)
                        .name
                        .iter()
                        .copied(),
                )
        };
        self.rank
            .cmp(&other.rank)
            .then_with(|| {
                if std::ptr::eq(self.directory, other.directory) {
                    self.item.name_key.cmp(&other.item.name_key)
                } else {
                    joined_key_cmp(
                        self.directory.path_key.as_bytes(),
                        self.item.name_key.as_bytes(),
                        other.directory.path_key.as_bytes(),
                        other.item.name_key.as_bytes(),
                    )
                }
            })
            .then_with(|| raw(self).cmp(raw(other)))
    }
}
impl PartialOrd for Match<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn retain_match<'a>(
    heap: &mut BinaryHeap<Match<'a>>,
    rank: usize,
    item: &'a Item,
    directory: &'a Directory,
) -> bool {
    let candidate = Match {
        rank,
        item,
        directory,
    };
    if heap.len() < RESULT_LIMIT {
        heap.push(candidate);
        true
    } else if heap.peek().is_some_and(|worst| candidate < *worst) {
        *heap.peek_mut().unwrap() = candidate;
        true
    } else {
        false
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
                    .partition_point(|item| item.name_key.as_ref() < term.as_str());
                let mut retained = 0;
                for (index, item) in directory.items[start..]
                    .iter()
                    .take_while(|item| item.name_key.starts_with(term))
                    .enumerate()
                {
                    if index % 256 == 0 && cancelled.load(Ordering::Relaxed) {
                        return None;
                    }
                    if hidden || !item.hidden() {
                        if !retain_match(
                            &mut best,
                            usize::from(item.name_key.as_ref() != term),
                            item,
                            directory,
                        ) {
                            // Later names in this parent have the same or worse
                            // rank and path order, so none can enter the heap.
                            break;
                        }
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
            let required = [required as u64, (required >> 64) as u64];
            let mut path_key = String::new();
            for directory in &directories {
                for (index, item) in directory.items.iter().enumerate() {
                    if index % 256 == 0 && cancelled.load(Ordering::Relaxed) {
                        return None;
                    }
                    let missing_letters =
                        (required[0] & !item.letters[0]) | (required[1] & !item.letters[1]);
                    if (!hidden && item.hidden()) || missing_letters != 0 {
                        continue;
                    }
                    path_key.clear();
                    path_key.push_str(&directory.path_key);
                    path_key.push_str(&item.name_key);
                    if let Some(rank) = score(&item.name_key, &path_key, &terms) {
                        retain_match(&mut best, rank, item, directory);
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
            let raw = result.directory.raw_item(item);
            let path = result.directory.item_path(item);
            let directory = raw.kind == 1 || (raw.kind == 2 && path.is_dir());
            entries.push(Entry {
                path,
                name: bytes_to_os(&raw.name)
                    .as_os_str()
                    .to_string_lossy()
                    .into_owned(),
                bytes: None,
                // Resolve only selected symlinks, never every file in the catalog.
                directory,
            });
        }
        Some(SearchResults {
            entries,
            skipped,
            finished,
        })
    }
}

// inotify is a Linux-only API: macOS and other unixes take the no-op watcher
// below, which polls on a timer instead of blocking on a native descriptor.
#[cfg(target_os = "linux")]
struct Watches {
    fd: Option<OwnedFd>,
    paths: HashMap<i32, PathBuf>,
}

#[cfg(target_os = "linux")]
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

    fn wait(&self) {
        if let Some(fd) = &self.fd {
            let mut descriptor = libc::pollfd {
                fd: fd.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            // Sleep until a filesystem notification arrives, with a bounded wait
            // for shutdown, cache checkpoints and mount reconciliation.
            // SAFETY: the owned fd and one initialized pollfd live through poll.
            unsafe {
                libc::poll(&mut descriptor, 1, 1000);
            }
        } else {
            thread::park_timeout(Duration::from_secs(1));
        }
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

#[cfg(not(target_os = "linux"))]
struct Watches;

#[cfg(not(target_os = "linux"))]
impl Watches {
    fn new() -> Self {
        Watches
    }

    fn add(&mut self, _path: &Path) {}

    fn remove_tree(&mut self, _root: &Path) {}

    fn wait(&self) {
        // No native notifications on this platform yet: poll on a timer.
        thread::park_timeout(Duration::from_secs(1));
    }

    fn changes(&mut self) -> (HashSet<PathBuf>, bool) {
        (HashSet::new(), false)
    }
}
struct Inventory {
    handle: SearchHandle,
    roots: Vec<PathBuf>,
    cache: Option<PathBuf>,
    watches: Watches,
    pending: VecDeque<PathBuf>,
    visited: HashSet<VisitIdentity>,
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
        #[cfg(unix)]
        {
            const PSEUDO: [&str; 3] = ["/proc", "/sys", "/dev"];
            if PSEUDO.iter().any(|root| path.starts_with(root)) {
                return true;
            }
        }
        #[cfg(windows)]
        {
            // Root noise that can never be worth crawling on Windows, matched
            // by directory name so drive-letter assumptions stay out. AppData
            // and node_modules alone bury real results under tens of thousands
            // of cache entries.
            const NOISE: [&str; 6] = [
                "AppData",
                "node_modules",
                "$Recycle.Bin",
                "System Volume Information",
                "Program Files",
                "ProgramData",
            ];
            let noisy = path.components().any(|component| {
                matches!(component, std::path::Component::Normal(name)
                    if NOISE.iter().any(|needle| name.eq_ignore_ascii_case(needle)))
            });
            if noisy || path.starts_with("\\\\?\\") {
                return true;
            }
        }
        self.cache
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
        #[cfg(unix)]
        let identity = (metadata.dev(), metadata.ino());
        #[cfg(windows)]
        let identity = windows_identity(&path);
        if !self.visited.insert(identity) {
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
                    name: os_name_bytes(&item.file_name()).into_boxed_slice(),
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
                previous.stored.stamp == stamp && previous.stored.items.as_ref() == stored
            }) {
                previous.clone()
            } else {
                let directory = Arc::new(Directory::new(
                    StoredDirectory {
                        path: path_bytes(path.as_path()),
                        stamp,
                        items: stored.into_boxed_slice(),
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
        if let Some(previous) = &previous
            && !Arc::ptr_eq(previous, &directory)
        {
            let children: HashSet<_> = directory
                .stored
                .items
                .iter()
                .filter(|item| item.kind == 1)
                .map(|item| &item.name)
                .collect();
            for child in previous
                .stored
                .items
                .iter()
                .filter(|item| item.kind == 1 && !children.contains(&item.name))
            {
                self.remove_tree(&path.join(bytes_to_os(&child.name).as_os_str()));
            }
        }
        let known = self.handle.0.read().unwrap();
        let children: Vec<_> = directory
            .stored
            .items
            .iter()
            .filter(|item| item.kind == 1)
            .map(|item| path.join(bytes_to_os(&item.name).as_os_str()))
            .filter(|path| self.full_scan || !known.directories.contains_key(path))
            .collect();
        drop(known);
        // Prepend children in reverse order to finish Home before other roots.
        // Avoid inserting in the middle of a large queue on every directory.
        for child in children.into_iter().rev() {
            self.pending.push_front(child);
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
        // These sets can contain millions of directory paths/inodes. Release
        // traversal-only allocations once reconciliation has finished.
        self.seen = HashSet::new();
        self.visited = HashSet::new();
        self.pending = VecDeque::new();
    }

    fn header(&self) -> (u32, Vec<Vec<u8>>) {
        (
            1,
            self.roots
                .iter()
                .map(|root: &std::path::PathBuf| path_bytes(root.as_path()))
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
            let path = PathBuf::from(bytes_to_os(&stored.path));
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
            #[cfg(unix)]
            if let Ok(metadata) = fs::metadata(&path) {
                let identity = (metadata.dev(), metadata.ino());
                self.visited.remove(&identity);
            }
            #[cfg(windows)]
            {
                let identity = windows_identity(&path);
                self.visited.remove(&identity);
            }
            if !self.pending.contains(&path) {
                self.pending.push_back(path);
            }
        }
    }

    fn run(&mut self, stopped: &AtomicBool) {
        // Cache failures are recoverable: always reconcile against the real filesystem.
        let _ = self.load(stopped);
        self.begin_scan(false);
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
                self.begin_scan(false);
            }
            self.watches.wait();
        }
    }
}

#[cfg(all(test, unix))]
#[path = "../../../tests/infrastructure/search_index.rs"]
mod tests;
