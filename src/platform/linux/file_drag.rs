//! Interoperable local file payloads for native outgoing drags.
use std::path::PathBuf;

pub(crate) fn uri_list(paths: &[PathBuf]) -> Option<String> {
    if paths.is_empty() {
        return None;
    }
    let mut uris = String::new();
    for path in paths {
        // ZIP members are virtual paths; they retain Virial's internal drag behavior.
        if !path.is_absolute() || !path.exists() {
            return None;
        }
        let uri = url::Url::from_file_path(path).ok()?;
        uris.push_str(uri.as_str());
        uris.push_str("\r\n");
    }
    Some(uris)
}

#[cfg(test)]
#[path = "../../../tests/platform/linux/file_drag.rs"]
mod tests;
