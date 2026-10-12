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

/// First four fields of a `$I` record: version (8 bytes), deleted file size
/// (8), deletion FILETIME (8), then the path length (4). Versions 1 and 2
/// share this prefix; only the path slot that follows changes size.
const HEADER: usize = 24;

/// The original path recorded in a `$I` metadata file; `None` when the record
/// is shorter than its header or carries a version we do not know.
fn parse_info(bytes: &[u8]) -> Option<PathBuf> {
    if bytes.len() < HEADER + 4 {
        return None;
    }
    // The version is stored as an u64, but only 1 and 2 are ever written.
    if u64::from(bytes[0]) > 2 {
        return None;
    }
    // The length counts UTF-16 code units, not bytes: the path is two bytes
    // per unit, and `min` keeps a truncated record from panicking.
    let units = usize::try_from(u32::from_le_bytes(bytes[24..28].try_into().ok()?)).ok()?;
    let end = (HEADER + 4 + units * 2).min(bytes.len());
    let path = String::from_utf16_lossy(
        &bytes[HEADER + 4..end]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>(),
    );
    Some(PathBuf::from(path))
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

/// The current user's SID, queried once. Without it the bin of the current
/// user cannot be located among the per-user folders of `$Recycle.Bin`.
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
        LocalFree(string as HLOCAL);
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
            let Some(original) = parse_info(&bytes) else {
                continue;
            };
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            entries.push(Entry {
                name: original
                    .file_name()
                    .unwrap_or(original.as_os_str())
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
        let destination = parse_info(&bytes).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Malformed recycle metadata")
        })?;
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
            if let Some(info) = info_of(source) {
                let _ = fs::remove_file(info);
            }
        }
        Ok(())
    })
}
