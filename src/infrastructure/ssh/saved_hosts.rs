//! Saved SSH hosts, persisted as JSON under `<data_home>/virial/ssh_hosts.json`.
//!
//! Follows the repository's persistence recipe used by workspaces and layout:
//! a process-wide write lock, a temporary file, then an atomic rename.
//! Secrets are never written: the stored auth hint only records *which method*
//! the host was configured with, so the dialog can preselect it.

use super::config::{AuthHint, HostConfig, HostId};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Mutex,
};

static WRITE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Serialize, Deserialize)]
struct SerializedHost {
    label: String,
    host: String,
    port: u16,
    username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    initial_path: Option<String>,
    auth: SerializedAuth,
}

#[derive(Serialize, Deserialize)]
enum SerializedAuth {
    Agent,
    Key { path: String },
    /// A secret is typed at connect time and never stored.
    Interactive,
}

fn serialize(host: &HostConfig) -> SerializedHost {
    SerializedHost {
        label: host.label.clone(),
        host: host.host.clone(),
        port: host.port,
        username: host.username.clone(),
        initial_path: host
            .initial_path
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned()),
        auth: match &host.auth_hint {
            AuthHint::Agent => SerializedAuth::Agent,
            AuthHint::Key { path } => SerializedAuth::Key {
                path: path.to_string_lossy().into_owned(),
            },
            AuthHint::Interactive => SerializedAuth::Interactive,
        },
    }
}

fn deserialize(saved: SerializedHost, id: HostId) -> HostConfig {
    HostConfig {
        id,
        label: saved.label,
        host: saved.host,
        port: saved.port,
        username: saved.username,
        initial_path: saved.initial_path.map(PathBuf::from),
        auth_hint: match saved.auth {
            SerializedAuth::Agent => AuthHint::Agent,
            SerializedAuth::Key { path } => AuthHint::Key { path: PathBuf::from(path) },
            SerializedAuth::Interactive => AuthHint::Interactive,
        },
    }
}

pub(crate) struct SavedHosts;

impl SavedHosts {
    /// Read the favorites list. Missing file means an empty list.
    pub(crate) fn read(data_home: &Path) -> io::Result<Vec<HostConfig>> {
        read_unlocked(&data_home.join("virial/ssh_hosts.json"))
    }

    /// Insert or replace a host (matched by id), preserving list order.
    pub(crate) fn upsert(data_home: &Path, host: HostConfig) -> io::Result<()> {
        let _guard = WRITE_LOCK
            .lock()
            .map_err(|_| io::Error::other("SSH hosts lock unavailable"))?;
        let path = data_home.join("virial/ssh_hosts.json");
        let mut hosts = read_unlocked(&path)?;
        if let Some(slot) = hosts.iter_mut().find(|saved| saved.id == host.id) {
            *slot = host;
        } else {
            hosts.push(host);
        }
        write_unlocked(&path, &hosts)
    }

    /// Remove a host by id. Removing an unknown id is not an error.
    pub(crate) fn remove(data_home: &Path, id: &HostId) -> io::Result<()> {
        let _guard = WRITE_LOCK
            .lock()
            .map_err(|_| io::Error::other("SSH hosts lock unavailable"))?;
        let path = data_home.join("virial/ssh_hosts.json");
        let mut hosts = read_unlocked(&path)?;
        hosts.retain(|host| &host.id != id);
        write_unlocked(&path, &hosts)
    }
}

fn read_unlocked(path: &Path) -> io::Result<Vec<HostConfig>> {
    let text = match fs::read_to_string(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        result => result?,
    };
    parse(&text)
}

fn write_unlocked(path: &Path, hosts: &[HostConfig]) -> io::Result<()> {
    let serialized: Vec<SerializedHost> = hosts.iter().map(serialize).collect();
    let text = serde_json::to_string_pretty(&serialized)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)?;
    }
    let temporary = path.with_extension(format!("json.{}", std::process::id()));
    fs::write(&temporary, text)?;
    fs::rename(&temporary, path)
}

fn parse(text: &str) -> io::Result<Vec<HostConfig>> {
    let saved: Vec<SerializedHost> = serde_json::from_str(text)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(saved
        .into_iter()
        .map(|item| {
            let id = HostId::new(&item.host, item.port, &item.username);
            deserialize(item, id)
        })
        .collect())
}

#[cfg(test)]
#[path = "../../../tests/infrastructure/ssh/saved_hosts.rs"]
mod tests;
