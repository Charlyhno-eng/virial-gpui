//! Identity of a remote host plus the credentials used to reach it.
//!
//! A [`HostConfig`] is the persisted unit (favorites list). [`HostId`] is the
//! compact key used by the session registry and by `Location::Remote`.
//! Secrets live only inside [`HostAuth`] and only for the lifetime of the
//! connect attempt; persistence stores an [`AuthHint`] instead.

use std::fmt;
use std::path::PathBuf;

/// Stable key for one configured host. Reconnection after a drop targets the
/// same id even if the user edited the port or username meanwhile.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct HostId(pub(crate) String);

impl HostId {
    pub(crate) fn new(host: &str, port: u16, username: &str) -> Self {
        Self(format!("{username}@{host}:{port}"))
    }
}

impl fmt::Display for HostId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// How a session authenticates. The typed secret variant is never persisted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HostAuth {
    /// Try the local SSH agent (SSH_AUTH_SOCK), then default key paths.
    Agent,
    /// A specific OpenSSH private key file, optionally protected by a passphrase.
    Key { path: PathBuf, passphrase: Option<String> },
    /// A secret typed at connect time and kept in memory only.
    Interactive(String),
}

impl HostAuth {
    /// Short human label for the connect dialog and tooltips.
    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::Agent => "SSH agent",
            Self::Key { .. } => "Private key",
            Self::Interactive(_) => "Password",
        }
    }
}

/// One saved remote host (a "favorite" in the connect dialog).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HostConfig {
    pub(crate) id: HostId,
    /// Label shown in the sidebar; defaults to the host name.
    pub(crate) label: String,
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) username: String,
    /// Remote directory opened on connect. `None` means the server default.
    pub(crate) initial_path: Option<PathBuf>,
    /// Non-secret auth method restored from disk.
    pub(crate) auth_hint: AuthHint,
}

impl HostConfig {
    pub(crate) fn new(host: &str, port: u16, username: &str) -> Self {
        Self {
            id: HostId::new(host, port, username),
            label: host.to_string(),
            host: host.to_string(),
            port,
            username: username.to_string(),
            initial_path: None,
            auth_hint: AuthHint::Agent,
        }
    }

    /// Identifier displayed to the user (e.g. `kali (demon@192.168.40.15:22)`).
    pub(crate) fn display(&self) -> String {
        format!("{} ({})", self.label, self.id.0)
    }
}

/// Persisted, secret-free summary of the authentication method of a host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AuthHint {
    Agent,
    Key { path: PathBuf },
    /// The secret is typed at connect time and never stored.
    Interactive,
}

#[cfg(test)]
#[path = "../../../tests/infrastructure/ssh/config.rs"]
mod tests;
