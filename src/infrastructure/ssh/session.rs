//! One live SSH connection and its SFTP operations.
//!
//! russh-sftp multiplexes requests by id, so the session shares one SFTP
//! session behind an `Arc` and lets callers issue concurrent operations.

use super::config::{HostAuth, HostConfig, HostId};
use crate::domain::models::Entry;
use russh::client::{self, AuthResult};
use russh::keys::{load_secret_key, PrivateKeyWithHashAlg};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex as AsyncMutex;

/// What the status bar badge shows. Plain data so the UI layer maps it to
/// colors and labels without knowing anything about russh.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SessionState {
    Connecting,
    Connected,
    Failed(String),
    Closed,
}

/// russh client handler: known_hosts verification with TOFU learning.
struct HostKeyVerifier {
    host: String,
    port: u16,
    /// Accept new host keys and record them (first-connection trust).
    learn: bool,
}

impl client::Handler for HostKeyVerifier {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        // The `ssh_key` re-export equals the `ssh_key::PublicKey` the checker
        // receives; known_hosts helpers take the same type.
        let recorded = russh::keys::check_known_hosts_path(
            &self.host,
            self.port,
            server_public_key,
            known_hosts_file(),
        );
        match recorded {
            Ok(true) => Ok(true),
            Ok(false) if self.learn => {
                // First contact: record the key and accept (TOFU).
                russh::keys::known_hosts::learn_known_hosts_path(
                    &self.host,
                    self.port,
                    server_public_key,
                    known_hosts_file(),
                )?;
                Ok(true)
            }
            Ok(false) => Ok(false),
            // Key changed for a host we already know: refuse (MITM protection).
            Err(_) => Ok(false),
        }
    }
}

fn known_hosts_file() -> std::path::PathBuf {
    // Redirect through HOME so tests can point the verifier at a scratch file
    // via XDG-style overrides; falls back to russh's own default when unset.
    if let Some(custom) = std::env::var_os("VIRIAL_KNOWN_HOSTS") {
        return PathBuf::from(custom);
    }
    match home::home_dir() {
        Some(dir) => dir.join(".ssh/known_hosts"),
        None => PathBuf::from(".ssh/known_hosts"),
    }
}

/// SFTP file type folded into the repository's own `Entry` model.
pub(crate) fn entry_from_dir_entry(
    parent: &Path,
    entry: russh_sftp::client::fs::DirEntry,
) -> Entry {
    entry_from_parts(
        parent,
        &entry.file_name(),
        entry.file_type().is_dir(),
        Some(entry.metadata().len()),
    )
}

/// Pure constructor used by `entry_from_dir_entry` and unit tests: mapping
/// remote metadata into the repository's own `Entry` model.
pub(crate) fn entry_from_parts(parent: &Path, name: &str, directory: bool, bytes: Option<u64>) -> Entry {
    Entry {
        path: parent.join(name),
        name: name.to_string(),
        directory,
        bytes,
    }
}

/// A live SSH session: connection handle plus an SFTP channel.
pub(crate) struct Session {
    pub(crate) id: HostId,
    pub(crate) state: SessionState,
    connection: AsyncMutex<client::Handle<HostKeyVerifier>>,
    sftp: Arc<AsyncMutex<russh_sftp::client::SftpSession>>,
}

impl Session {
    /// Establish the connection, authenticate, and open the SFTP channel.
    pub(crate) async fn connect(config: HostConfig, auth: HostAuth) -> Result<Self, String> {
        Self::establish(config, auth).await
    }

    async fn establish(config: HostConfig, auth: HostAuth) -> Result<Self, String> {
        let verifier = HostKeyVerifier {
            host: config.host.clone(),
            port: config.port,
            learn: true,
        };
        let client_config = client::Config {
            inactivity_timeout: None,
            keepalive_interval: Some(std::time::Duration::from_secs(15)),
            keepalive_max: 3,
            ..Default::default()
        };
        let address = (config.host.as_str(), config.port);
        let mut handle = client::connect(Arc::new(client_config), address, verifier)
            .await
            .map_err(|error| format!("{}:{}, {error}", config.host, config.port))?;
        let username = config.username.clone();
        let outcome = match &auth {
            HostAuth::Agent => Self::authenticate_with_agent(&mut handle, &username).await,
            HostAuth::Key { path, passphrase } => {
                // `load_secret_key` is synchronous in russh 0.62.
                let key = load_secret_key(path, passphrase.as_deref())
                    .map_err(|error| format!("Cannot read key {path:?}: {error}"))?;
                let key = PrivateKeyWithHashAlg::new(Arc::new(key), None);
                handle
                    .authenticate_publickey(username, key)
                    .await
                    .map_err(|error| format!("Authentication failed: {error}"))
            }
            HostAuth::Interactive(secret) => {
                handle
                    .authenticate_password(username, secret.clone())
                    .await
                    .map_err(|error| format!("Authentication failed: {error}"))
            }
        };
        match outcome {
            Ok(result) if result.success() => {}
            Ok(_) => return Err("Authentication rejected by the remote host".into()),
            Err(error) => return Err(error),
        }
        let channel = handle
            .channel_open_session()
            .await
            .map_err(|error| format!("Cannot open SFTP channel: {error}"))?;
        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|error| format!("SFTP subsystem unavailable: {error}"))?;
        let sftp = russh_sftp::client::SftpSession::new(channel.into_stream())
            .await
            .map_err(|error| format!("SFTP initialization failed: {error}"))?;
        Ok(Self {
            id: config.id.clone(),
            state: SessionState::Connected,
            connection: AsyncMutex::new(handle),
            sftp: Arc::new(AsyncMutex::new(sftp)),
        })
    }

    /// Enumerate agent identities and try each one (ssh-agent flow).
    async fn authenticate_with_agent(
        handle: &mut client::Handle<HostKeyVerifier>,
        username: &str,
    ) -> Result<AuthResult, String> {
        #[cfg(unix)]
        {
            let mut agent = russh::keys::agent::client::AgentClient::connect_env()
                .await
                .map_err(|error| format!("No SSH agent: {error}"))?;
            let identities = agent
                .request_identities()
                .await
                .map_err(|error| format!("SSH agent error: {error}"))?;
            let mut last = Err("SSH agent has no identities".into());
            for identity in identities {
                let public = identity.public_key().into_owned();
                match handle
                    .authenticate_publickey_with(username, public, None, &mut agent)
                    .await
                {
                    Ok(result) if result.success() => return Ok(result),
                    Ok(_) => last = Err("SSH agent key rejected".into()),
                    Err(error) => last = Err(format!("SSH agent error: {error}")),
                }
            }
            last
        }
        #[cfg(not(unix))]
        {
            let _ = handle;
            let _ = username;
            Err("SSH agent authentication is unavailable on this platform".into())
        }
    }

    pub(crate) fn state(&self) -> &SessionState {
        &self.state
    }

    pub(crate) fn set_state(&mut self, state: SessionState) {
        self.state = state;
    }

    /// Canonical absolute path of a remote directory.
    pub(crate) async fn canonicalize(&self, path: &str) -> Result<String, String> {
        self.sftp
            .lock()
            .await
            .canonicalize(path)
            .await
            .map_err(|error| format!("Cannot resolve {path}: {error}"))
    }

    /// List a remote directory as the repository's own entries.
    pub(crate) async fn read_dir(&self, directory: &Path) -> Result<Vec<Entry>, String> {
        let path = directory.to_string_lossy().into_owned();
        let sftp = self.sftp.lock().await;
        let read = sftp
            .read_dir(&path)
            .await
            .map_err(|error| format!("Cannot read {path}: {error}"))?;
        Ok(read
            .map(|entry| entry_from_dir_entry(directory.as_ref() as &Path, entry))
            .collect())
    }

    /// Create a remote directory.
    pub(crate) async fn create_dir(&self, path: &Path) -> Result<(), String> {
        let target = path.to_string_lossy().into_owned();
        self.sftp
            .lock()
            .await
            .create_dir(&target)
            .await
            .map_err(|error| format!("Cannot create {target}: {error}"))
    }

    /// Rename (move) a remote path.
    pub(crate) async fn rename(&self, from: &Path, to: &Path) -> Result<(), String> {
        let (source, target) = (
            from.to_string_lossy().into_owned(),
            to.to_string_lossy().into_owned(),
        );
        self.sftp
            .lock()
            .await
            .rename(&source, &target)
            .await
            .map_err(|error| format!("Cannot rename {source}: {error}"))
    }

    /// Delete a remote file.
    pub(crate) async fn remove_file(&self, path: &Path) -> Result<(), String> {
        let target = path.to_string_lossy().into_owned();
        self.sftp
            .lock()
            .await
            .remove_file(&target)
            .await
            .map_err(|error| format!("Cannot delete {target}: {error}"))
    }

    /// Delete an empty remote directory.
    pub(crate) async fn remove_dir(&self, path: &Path) -> Result<(), String> {
        let target = path.to_string_lossy().into_owned();
        self.sftp
            .lock()
            .await
            .remove_dir(&target)
            .await
            .map_err(|error| format!("Cannot delete {target}: {error}"))
    }

    /// Read a whole remote file into memory (small files: previews).
    pub(crate) async fn read_file(&self, path: &Path) -> Result<Vec<u8>, String> {
        let target = path.to_string_lossy().into_owned();
        self.sftp
            .lock()
            .await
            .read(&target)
            .await
            .map_err(|error| format!("Cannot read {target}: {error}"))
    }

    // TODO(ssh-pr3): stream large files with open() + chunked reads instead
    // of read(), reporting progress through infrastructure::progress.
    // TODO(ssh-pr2): expose the presented host key fingerprint after the
    // handshake so the connect dialog can show it (verify dialog).

    /// Close the session gracefully.
    pub(crate) async fn disconnect(&self) {
        let mut handle = self.connection.lock().await;
        let _ = handle
            .disconnect(russh::Disconnect::ByApplication, "", "english")
            .await;
    }
}
