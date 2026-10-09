//! Read desktop XBEL history and maintain a separate, bounded Virial history.
use crate::domain::models::Entry;
use std::{
    collections::HashSet,
    fs, io,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use url::Url;

static WRITE_LOCK: Mutex<()> = Mutex::new(());
const LIMIT: usize = 200;

pub fn data_home(home: &Path) -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".local/share"))
}

fn optional_read(path: &Path) -> io::Result<String> {
    match fs::read_to_string(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        result => result,
    }
}

fn own_records(text: &str) -> Vec<(i64, PathBuf)> {
    text.lines()
        .filter_map(|line| {
            let (timestamp, uri) = line.split_once('\t')?;
            Some((
                timestamp.parse().ok()?,
                Url::parse(uri).ok()?.to_file_path().ok()?,
            ))
        })
        .collect()
}

fn desktop_records(text: &str) -> io::Result<Vec<(i64, PathBuf)>> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let document = roxmltree::Document::parse(text)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if !document.root_element().has_tag_name("xbel") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Expected an XBEL document",
        ));
    }
    Ok(document
        .root_element()
        .children()
        .filter(|node| node.has_tag_name("bookmark"))
        .filter_map(|node| {
            let path = Url::parse(node.attribute("href")?)
                .ok()?
                .to_file_path()
                .ok()?;
            let timestamp = ["visited", "modified", "added"]
                .iter()
                .filter_map(|key| {
                    chrono::DateTime::parse_from_rfc3339(node.attribute(*key)?)
                        .ok()
                        .map(|date| date.timestamp_micros())
                })
                .max()
                .unwrap_or(0);
            Some((timestamp, path))
        })
        .collect())
}

pub fn read(data: &Path, hidden: bool) -> io::Result<Vec<Entry>> {
    let mut records = desktop_records(&optional_read(&data.join("recently-used.xbel"))?)?;
    records.extend(own_records(&optional_read(
        &data.join("virial/recent-files"),
    )?));
    records.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let mut seen = HashSet::new();
    Ok(records
        .into_iter()
        .filter_map(|(_, path)| {
            if !seen.insert(path.clone()) {
                return None;
            }
            let name = path.file_name()?.to_string_lossy().into_owned();
            if !hidden && name.starts_with('.') {
                return None;
            }
            if super::archive::is_member(&path) {
                return super::archive::entry(&path).ok();
            }
            let metadata = fs::metadata(&path).ok()?;
            Some(Entry {
                path,
                name,
                directory: metadata.is_dir(),
                bytes: metadata.is_file().then_some(metadata.len()),
            })
        })
        .take(LIMIT)
        .collect())
}

pub fn record(data: &Path, path: &Path) -> io::Result<()> {
    let _guard = WRITE_LOCK
        .lock()
        .map_err(|_| io::Error::other("Recent history lock unavailable"))?;
    let directory = data.join("virial");
    let target = directory.join("recent-files");
    let mut records = own_records(&optional_read(&target)?);
    records.retain(|(_, previous)| previous != path);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros()
        .min(i64::MAX as u128) as i64;
    records.push((timestamp, path.to_path_buf()));
    records.sort_by_key(|&(timestamp, _)| std::cmp::Reverse(timestamp));
    let text: String = records
        .into_iter()
        .take(LIMIT)
        .filter_map(|(timestamp, path)| {
            Url::from_file_path(path)
                .ok()
                .map(|uri| format!("{timestamp}\t{uri}\n"))
        })
        .collect();
    fs::create_dir_all(&directory)?;
    let temporary = directory.join(format!("recent-files.{}.tmp", std::process::id()));
    fs::write(&temporary, text)?;
    fs::rename(&temporary, target)
}

#[cfg(test)]
#[path = "../../tests/infrastructure/recent.rs"]
mod tests;
