//! The Windows Recycle Bin, so View → Trash works like on Linux.
//!
//! Items live in `<volume>\$Recycle.Bin\<SID>\` as `$R<name>` (the data) plus
//! `$I<name>` (the metadata). The $I header carries the original path and the
//! deletion time, which is all this module needs: enumeration is plain
//! directory walking and restore is a rename of `$R` back to the original.
//! The COM `IShellItem` route would add a lifecycle for no extra information.
//!
//! Everything here runs on a worker thread; the UI only sees `Entry` lists and
//! `io::Result`.

use crate::domain::models::Entry;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// File size of a version-1 $I header (volume 8 bytes, file size 8, FILETIME
/// 8, path length 4) plus the fixed 520-byte UTF-16 path slot: version 2
/// reuses the same layout with a longer path slot, so both parse identically.
const HEADER: usize = 24;
const SLOT: usize = 520;

/// One parsed $I record: where the item came from and when it was deleted.
struct Info {
    original: PathBuf,
    deleted_ms: u64,
}

/// Read the metadata of a trash item; `None` for a malformed record.
fn parse_info(bytes: &[u8]) -> Option<Info> {
    let version = u64::from(bytes[0]);
    // Versions beyond the known two are not guaranteed to keep the layout.
    if version > 2 {
        return None;
    }
    let size = u64::from_le_bytes(bytes[8..16].try_into().ok()?);
    let _ = size;
    let filetime = u64::from_le_bytes(bytes[16..24].try_into().ok()?);
    // Windows FILETIME: 100 ns ticks since 1601-01-01.
    let deleted_ms = filetime / 10_000 - 11_644_473_600_000;
    let len = usize::try_from(u32::from_le_bytes(bytes[24..28].try_into().ok()?)).ok()?;
    let end = (HEADER + 4 + len).min(bytes.len());
    let path = String::from_utf16_lossy(
        bytes[HEADER + 4..end]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes(pair.try_into().ok()?))
            .collect::<Vec<_>>()
            .as_slice(),
    );
    Some(Info {
        original: PathBuf::from(path),
        deleted_ms,
    })
}

/// `$R<name>` → `$I<name>`: the metadata twin of a data file.
fn info_of(data_file: &Path) -> Option<PathBuf> {
    let name = data_file.file_name()?.to_str()?;
    let twin = name.replacen('R', "I", 1);
    Some(data_file.with_file_name(twin))
}

/// The per-user trash folder of every fixed volume that has one.
fn bins() -> Vec<PathBuf> {
    let sid = whoami_sid();
    let mut roots = Vec::new();
    for letter in 'C'..='Z' {
        let volume = PathBuf::from(format!("{letter}:\\"));
        if !volume.is_dir() {
            continue;
        }
        let bin = volume.join("$Recycle.Bin").join(&sid);
        if bin.is_dir() {
            roots.push(bin);
        }
    }
    roots
}

/// The current user's SID, queried once. Without it the other users' bins are
/// invisible, and `GetUserProfileDirectory` needs a token we get here.
fn whoami_sid() -> String {
    thread_local! {
        static SID: String = sid_once().unwrap_or_default();
    }
    SID.with(|sid| sid.clone())
}

fn sid_once() -> Option<String> {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree};
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidA;
    use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return None;
        }
        let mut needed = 0u32;
        let _ = GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut needed);
        if needed == 0 {
            CloseHandle(token);
            return None;
        }
        let mut buffer = vec![0u8; needed as usize];
        if GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            needed,
            &mut needed,
        ) == 0
        {
            CloseHandle(token);
            return None;
        }
        CloseHandle(token);
        let user = buffer.as_ptr().cast::<TOKEN_USER>();
        let sid = (*user).User.Sid;
        // The ANSI conversion allocates; give it back through the local
        // allocator by copying before the free.
        let mut string: *mut u8 = std::ptr::null_mut();
        if ConvertSidToStringSidA(sid, &mut string) == 0 {
            return None;
        }
        let mut len = 0usize;
        while *string.add(len) != 0 {
            len += 1;
        }
        let text = String::from_utf8_lossy(std::slice::from_raw_parts(string, len)).into_owned();
        LocalFree(HLOCAL(string.cast()));
        Some(text)
    }
}

/// List the trash: every `$R*` with a readable `$I*` twin becomes an entry.
pub(crate) fn read(_data: &Path) -> io::Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for bin in bins() {
        let files = match fs::read_dir(&bin) {
            Ok(files) => files,
            // Another user's bin or a locked volume: skip quietly.
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => continue,
            Err(error) => return Err(error),
        };
        for file in files {
            let file = match file {
                Ok(file) => file,
                Err(_) => continue,
            };
            let path = file.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !name.starts_with("$R") {
                continue;
            }
            let Some(info_path) = info_of(&path) else {
                continue;
            };
            let Ok(bytes) = fs::read(&info_path) else {
                continue;
            };
            let Some(info) = parse_info(&bytes) else {
                continue;
            };
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            let _ = info.deleted_ms;
            entries.push(Entry {
                name: info
                    .original
                    .file_name()
                    .unwrap_or(info.original.as_os_str())
                    .to_string_lossy()
                    .into(),
                path,
                directory: metadata.is_dir(),
                bytes: metadata.is_file().then_some(metadata.len()),
            });
        }
    }
    entries.sort_by_cached_key(|entry| (!entry.directory, entry.name.to_lowercase()));
    Ok(entries)
}

/// Empty every owned bin. Entries whose deletion fails keep their metadata.
pub(crate) fn empty(_data: &Path) -> io::Result<()> {
    let mut first_error = None;
    for bin in bins() {
        let files = match fs::read_dir(&bin) {
            Ok(files) => files,
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => continue,
            Err(error) => return Err(error),
        };
        for file in files {
            let file = match file {
                Ok(file) => file,
                Err(_) => continue,
            };
            let path = file.path();
            let name = path.file_name().map(|name| name.to_os_string());
            let Some(name) = name else { continue };
            if !name.as_encoded_bytes().starts_with(b"$R") {
                continue;
            }
            if let Err(error) = super::operations::remove(&path) {
                first_error.get_or_insert(error);
                continue;
            }
            if let Some(info) = info_of(&path) {
                let _ = fs::remove_file(info);
            }
        }
    }
    first_error.map_or(Ok(()), Err)
}

/// Restore items to their recorded original paths, recorded as one undo step.
pub(crate) fn restore(_data: &Path, paths: &[PathBuf]) -> io::Result<()> {
    // Collect the moves first: one malformed record aborts before any change.
    let mut moves = Vec::new();
    for path in paths {
        let Some(info_path) = info_of(path) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Not a Recycle Bin item",
            ));
        };
        let bytes = fs::read(&info_path)?;
        let info = parse_info(&bytes).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Malformed recycle metadata")
        })?;
        let destination = info.original;
        if destination.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} already exists", destination.display()),
            ));
        }
        if moves
            .iter()
            .any(|(_, existing): &(PathBuf, PathBuf)| existing == &destination)
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Selected items have the same original path",
            ));
        }
        moves.push((path.clone(), destination));
    }
    if moves.is_empty() {
        return Ok(());
    }
    let affected: Vec<PathBuf> = moves
        .iter()
        .flat_map(|(source, destination)| [source.clone(), destination.clone()])
        .collect();
    // One selection is one undoable action, like the desktop trash on Linux.
    super::undo::record(_data, affected, || {
        for (source, destination) in &moves {
            super::operations::rename(source, destination)?;
            let _ = fs::remove_file(info_of(source).unwrap_or_default());
        }
        Ok(())
    })
}
