//! Portable fallback for outgoing native drag payloads.
use std::path::PathBuf;

pub(crate) fn uri_list(paths: &[PathBuf]) -> Option<String> {
    if paths.is_empty() {
        return None;
    }
    let mut uris = String::new();
    for path in paths {
        if !path.is_absolute() || !path.exists() {
            return None;
        }
        let uri = url::Url::from_file_path(path).ok()?;
        uris.push_str(uri.as_str());
        uris.push_str("\r\n");
    }
    Some(uris)
}
