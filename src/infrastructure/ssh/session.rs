//! One live SSH connection and its SFTP operations.
//!
//! russh-sftp multiplexes requests by id, so the session shares one SFTP
//! session behind an `Arc` and lets callers issue concurrent operations.

use super::config::{HostAuth, HostConfig};
use crate::domain::models::Entry;
use russh::client::{self, AuthResult};
use russh::keys::{PrivateKeyWithHashAlg, load_secret_key};
use std::marker::Unpin;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex as AsyncMutex;

/// What the status bar badge shows. Plain data so the UI layer maps it to
/// colors and labels without knowing anything about russh.
///
/// Only `Connected` is constructed today: the store never keeps a session in
/// another state (a failed connect simply returns an error). The remaining
/// variants are the badge contract reserved by TODO(ssh-pr2) (store.rs), which
/// will surface the in-flight and failed states in the UI.
#[allow(dead_code)]
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
pub(crate) fn entry_from_parts(
    parent: &Path,
    name: &str,
    directory: bool,
    bytes: Option<u64>,
) -> Entry {
    Entry {
        path: join_remote(parent, name),
        name: name.to_string(),
        directory,
        bytes,
    }
}

/// Join a remote POSIX parent and name. Remote paths must never go through
/// `Path::join`: on Windows it injects `\` separators the SFTP server rejects
/// (`/home\demon`). Passing through here also repairs paths that arrived via
/// a Windows `PathBuf`.
pub(crate) fn join_remote(parent: &Path, name: &str) -> PathBuf {
    let base = parent.to_string_lossy().replace('\\', "/");
    let base = base.trim_end_matches('/');
    if base.is_empty() {
        PathBuf::from(format!("/{name}"))
    } else {
        PathBuf::from(format!("{base}/{name}"))
    }
}

/// The wire form of a remote path: forward slashes only.
fn remote_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Enumerate one agent connection's identities and try each against `handle`.
async fn try_agent_identities<S: russh::keys::agent::client::AgentStream + Send + Unpin>(
    mut agent: russh::keys::agent::client::AgentClient<S>,
    handle: &mut client::Handle<HostKeyVerifier>,
    username: &str,
) -> Result<AuthResult, String> {
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

/// A live SSH session: connection handle plus an SFTP channel.
pub(crate) struct Session {
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
        // A filtered port otherwise burns ~21 s of SYN retries with the badge
        // stuck on "Connecting…"; 10 s keeps the failure actionable.
        let mut handle = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            client::connect(Arc::new(client_config), address, verifier),
        )
        .await
        .map_err(|_| {
            format!(
                "{}:{}: connection timed out after 10 s",
                config.host, config.port
            )
        })?
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
            HostAuth::Interactive(secret) => handle
                .authenticate_password(username, secret.clone())
                .await
                .map_err(|error| format!("Authentication failed: {error}")),
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
            state: SessionState::Connected,
            connection: AsyncMutex::new(handle),
            sftp: Arc::new(AsyncMutex::new(sftp)),
        })
    }

    /// Enumerate agent identities and try each one (ssh-agent flow). When no
    /// agent has a usable key, the default keys in `~/.ssh` are tried — which
    /// is what the connect dialog promises, and what makes a plain key-based
    /// connect work without any agent running.
    async fn authenticate_with_agent(
        handle: &mut client::Handle<HostKeyVerifier>,
        username: &str,
    ) -> Result<AuthResult, String> {
        #[cfg(unix)]
        let last = match russh::keys::agent::client::AgentClient::connect_env().await {
            Ok(agent) => try_agent_identities(agent, handle, username).await,
            Err(error) => Err(format!("No SSH agent: {error}")),
        };
        #[cfg(windows)]
        // Pageant first, then the OpenSSH agent's named pipe; the two streams
        // are distinct types, so each connection scopes its own attempt.
        let last = match russh::keys::agent::client::AgentClient::connect_pageant().await {
            Ok(agent) => try_agent_identities(agent, handle, username).await,
            Err(_) => {
                match russh::keys::agent::client::AgentClient::connect_named_pipe(
                    r"\\.\pipe\openssh-ssh-agent",
                )
                .await
                {
                    Ok(agent) => try_agent_identities(agent, handle, username).await,
                    Err(error) => Err(format!("No SSH agent: {error}")),
                }
            }
        };
        if last.is_ok() {
            return last;
        }
        // No agent key worked (or no agent at all): fall back to the default
        // keys, which is what the connect dialog promises.
        if let Some(home) = home::home_dir() {
            for name in ["id_ed25519", "id_rsa"] {
                let path = home.join(".ssh").join(name);
                if !path.exists() {
                    continue;
                }
                let Ok(key) = load_secret_key(&path, None) else {
                    continue;
                };
                let key = PrivateKeyWithHashAlg::new(Arc::new(key), None);
                match handle.authenticate_publickey(username, key).await {
                    Ok(result) if result.success() => return Ok(result),
                    _ => {}
                }
            }
        }
        last
    }

    pub(crate) fn state(&self) -> &SessionState {
        &self.state
    }

    /// List a remote directory as the repository's own entries.
    pub(crate) async fn read_dir(&self, directory: &Path) -> Result<Vec<Entry>, String> {
        let path = remote_string(directory);
        let sftp = self.sftp.lock().await;
        let read = sftp
            .read_dir(&path)
            .await
            .map_err(|error| format!("Cannot read {path}: {error}"))?;
        Ok(read
            .map(|entry| entry_from_dir_entry(directory as &Path, entry))
            .collect())
    }

    /// Create a remote directory.
    pub(crate) async fn create_dir(&self, path: &Path) -> Result<(), String> {
        let target = remote_string(path);
        self.sftp
            .lock()
            .await
            .create_dir(&target)
            .await
            .map_err(|error| format!("Cannot create {target}: {error}"))
    }

    /// Rename (move) a remote path.
    pub(crate) async fn rename(&self, from: &Path, to: &Path) -> Result<(), String> {
        let (source, target) = (remote_string(from), remote_string(to));
        self.sftp
            .lock()
            .await
            .rename(&source, &target)
            .await
            .map_err(|error| format!("Cannot rename {source}: {error}"))
    }

    /// Delete a remote file.
    pub(crate) async fn remove_file(&self, path: &Path) -> Result<(), String> {
        let target = remote_string(path);
        self.sftp
            .lock()
            .await
            .remove_file(&target)
            .await
            .map_err(|error| format!("Cannot delete {target}: {error}"))
    }

    /// Delete an empty remote directory.
    pub(crate) async fn remove_dir(&self, path: &Path) -> Result<(), String> {
        let target = remote_string(path);
        self.sftp
            .lock()
            .await
            .remove_dir(&target)
            .await
            .map_err(|error| format!("Cannot delete {target}: {error}"))
    }

    /// Read a whole remote file into memory (small files: previews).
    pub(crate) async fn read_file(&self, path: &Path) -> Result<Vec<u8>, String> {
        let target = remote_string(path);
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
        let handle = self.connection.lock().await;
        let _ = handle
            .disconnect(russh::Disconnect::ByApplication, "", "english")
            .await;
    }
}

#[cfg(test)]
#[path = "../../../tests/infrastructure/ssh/session.rs"]
mod tests;
