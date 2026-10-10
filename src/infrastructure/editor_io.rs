//! Byte-level file access for the text editor: line endings, byte order mark,
//! encoding and conflict detection, kept off the UI thread by the caller.
//!
//! The buffer is always UTF-8 internally. What changes on save is only the
//! wrapper: BOM, and CRLF when the file was already CRLF on disk.

use std::io::Write;
use std::path::Path;

/// Line ending the file uses on disk, remembered so a save round-trips.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    LfCr,
    Cr,
}

impl LineEnding {
    /// Guess the dominant ending, or Lf when the file is single-line.
    pub fn detect(text: &str) -> Self {
        let crlf = text.matches("\r\n").count();
        let lf = text.matches('\n').count();
        let cr = text.matches('\r').count();
        if crlf > 0 && crlf * 2 >= lf + cr {
            Self::LfCr
        } else if cr > 0 && cr * 2 >= lf + cr {
            Self::Cr
        } else {
            Self::Lf
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::LfCr => "\r\n",
            Self::Cr => "\r",
        }
    }
}

/// Everything a save needs to reproduce the original file byte for byte.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentFormat {
    pub line_ending: LineEnding,
    pub bom: bool,
    /// The file was not valid UTF-8 and was read as Latin-1.
    pub latin1_fallback: bool,
}

/// Decode file bytes into UTF-8 text plus the format needed to write it back.
pub fn decode(bytes: &[u8]) -> Option<(String, DocumentFormat)> {
    let (body, bom) = match bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
        Some(body) => (body, true),
        None => (bytes, false),
    };
    let (text, latin1_fallback) = match std::str::from_utf8(body) {
        Ok(text) => (text.to_owned(), false),
        // Latin-1 maps every byte to exactly one character, so a save never
        // loses data. A NUL byte means binary, not text: refuse it.
        Err(_) if body.contains(&0) => return None,
        Err(_) => (
            body.iter().map(|byte| char::from(*byte)).collect::<String>(),
            true,
        ),
    };
    Some((
        text,
        DocumentFormat {
            line_ending: LineEnding::detect(&text),
            bom,
            latin1_fallback,
        },
    ))
}

/// Encode UTF-8 text back to disk bytes in the document's original format.
pub fn encode(text: &str, format: &DocumentFormat) -> Vec<u8> {
    let normalized = match format.line_ending {
        LineEnding::Lf => text.replace("\r\n", "\n").replace('\r', "\n"),
        LineEnding::LfCr => text
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .replace('\n', "\r\n"),
        LineEnding::Cr => text
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .replace('\n', "\r"),
    };
    let mut bytes = Vec::with_capacity(normalized.len() + 3);
    if format.bom {
        bytes.extend_from_slice(&[0xef, 0xbb, 0xbf]);
    }
    bytes.extend_from_slice(normalized.as_bytes());
    bytes
}

/// A cheap fingerprint of what is on disk, to detect edits made elsewhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fingerprint {
    pub len: u64,
    /// Seconds since the epoch: unaffected by sub-second writes.
    pub modified: Option<u64>,
    /// FNV-1a over the content, so a same-length same-second rewrite is caught.
    pub hash: u64,
}

pub fn fingerprint(bytes: &[u8], modified: Option<std::time::SystemTime>) -> Fingerprint {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    Fingerprint {
        len: bytes.len() as u64,
        modified: modified.and_then(|time| {
            time.duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|since| since.as_secs())
        }),
        hash,
    }
}

/// Write through a sibling temporary file so a failure never truncates the
/// original, then rename over it. Permissions of the original are kept.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    // Prefer a sibling temporary: rename() across filesystems fails.
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
    let temporary = match parent {
        Some(parent) => parent.join(format!(".virial-editor-{}.tmp", std::process::id())),
        None => path.with_extension("virial.tmp"),
    };
    let permissions = std::fs::metadata(path).ok().map(|m| m.permissions());
    let mut file = std::fs::File::create(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    if let Some(permissions) = permissions.as_ref() {
        let _ = std::fs::set_permissions(&temporary, permissions.clone());
    }
    // Windows refuses rename over an existing file.
    if cfg!(windows) {
        let _ = std::fs::remove_file(path);
    }
    std::fs::rename(&temporary, path)?;
    if let Some(permissions) = permissions {
        let _ = std::fs::set_permissions(path, permissions);
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/infrastructure/editor_io.rs"]
mod tests;