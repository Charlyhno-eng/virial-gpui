//! GPUI-side SSH state: the bridge between the transport core
//! (`infrastructure::ssh`) and the UI (status bar badge, sidebar, dialogs).
//!
//! Async transport work runs on GPUI's background executor; results come back
//! through `cx.spawn` exactly like the device monitor in `state/devices.rs`.
//! The tokio-based store is driven through a dedicated runtime because GPUI's
//! executor is not a tokio runtime.

use crate::infrastructure::ssh::{
    AuthHint, ConnectOutcome, HostAuth, HostConfig, HostId, SavedHosts, SshStore,
};
use gpui::{Context, Task};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::runtime::{Builder, Runtime};

/// What the status-bar indicator and the connect dialog reflect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SshActivity {
    /// No remote session: the indicator shows the plain remote glyph.
    Idle,
    /// A connect attempt is running for this host key (`user@host:port`).
    Connecting(String),
    /// At least one session is connected; carries the newest active host id.
    Connected(HostId),
}

pub(crate) struct SshManager {
    pub(crate) store: Arc<SshStore>,
    /// Tokio runtime driving russh futures; created once, never recreated.
    pub(crate) runtime: Arc<Runtime>,
    pub(crate) hosts: Vec<HostConfig>,
    pub(crate) activity: SshActivity,
    pub(crate) error: Option<String>,
    /// Guards against stacking connect attempts for the same host.
    pub(crate) pending: Option<Task<()>>,
    /// Remote menu opened from the status-bar indicator.
    pub(crate) menu_open: bool,
    /// The connect dialog with its input, when shown.
    pub(crate) dialog_input: Option<gpui::Entity<crate::ui::components::input::NameInput>>,
    /// Credential typed into the dialog for the pending connect.
    pub(crate) dialog_credential: Option<String>,
}

impl SshManager {
    pub(crate) fn new(data_home: &std::path::Path) -> Self {
        let runtime = Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("tokio runtime for SSH");
        Self {
            store: Arc::new(SshStore::new()),
            runtime: Arc::new(runtime),
            hosts: SavedHosts::read(data_home).unwrap_or_default(),
            activity: SshActivity::Idle,
            error: None,
            pending: None,
            menu_open: false,
            dialog_input: None,
            dialog_credential: None,
        }
    }

    pub(crate) fn connected(&self, id: &HostId) -> bool {
        matches!(&self.activity, SshActivity::Connected(active) if active == id)
    }

    /// Refresh the saved-host list from disk (after dialog edits).
    pub(crate) fn reload_hosts(&mut self, data_home: &std::path::Path) {
        self.hosts = SavedHosts::read(data_home)
            .unwrap_or_else(|_| std::mem::take(&mut self.hosts));
    }

    /// Build the auth request for a host, honoring its saved hint. The typed
    /// credential (when the dialog collected one) is consumed here in memory
    /// and never leaves this function alive into persistence.
    pub(crate) fn auth_for(host: &HostConfig, typed_credential: Option<String>) -> HostAuth {
        match (&host.auth_hint, typed_credential) {
            (AuthHint::Agent, _) => HostAuth::Agent,
            (AuthHint::Key { path }, passphrase) => HostAuth::Key {
                path: path.clone(),
                passphrase,
            },
            (AuthHint::Interactive, Some(credential)) => HostAuth::Interactive(credential),
            (AuthHint::Interactive, None) => HostAuth::Agent,
        }
    }

    /// Parse the "user@host:port" or "user@host" shorthand from the dialog.
    pub(crate) fn parse_target(text: &str) -> Result<HostConfig, String> {
        let text = text.trim();
        let (user_part, host_part) = match text.split_once('@') {
            Some((user, rest)) if !user.is_empty() => (user, rest),
            _ => return Err("Expected user@host[:port]".into()),
        };
        let (host, port) = match host_part.rsplit_once(':') {
            Some((host, port)) => match port.parse::<u16>() {
                Ok(port) if port > 0 => (host, port),
                _ => return Err("Port must be 1-65535".into()),
            },
            None => (host_part, 22),
        };
        if host.is_empty() {
            return Err("Host must not be empty".into());
        }
        Ok(HostConfig::new(host, port, user_part))
    }

    /// Launch a connect attempt on the background runtime, updating
    /// `activity` on the UI thread at start and completion.
    pub(crate) fn spawn_connect(
        &mut self,
        host: HostConfig,
        credential: Option<String>,
        data_home: PathBuf,
        cx: &mut Context<crate::app::FileManager>,
    ) {
        if self.pending.is_some() {
            return;
        }
        self.error = None;
        self.activity = SshActivity::Connecting(host.id.to_string());
        // Persist the favorite (without any credential) so it reappears
        // in the dialog next time.
        if !self.hosts.iter().any(|saved| saved.id == host.id) {
            let mut favorite = host.clone();
            favorite.auth_hint = match &favorite.auth_hint {
                AuthHint::Interactive => AuthHint::Interactive,
                other => other.clone(),
            };
            if SavedHosts::upsert(&data_home, favorite).is_ok() {
                self.reload_hosts(&data_home);
            }
        }
        let auth = Self::auth_for(&host, credential);
        let store = self.store.clone();
        let runtime = self.runtime.clone();
        let id = host.id.clone();
        self.pending = Some(cx.spawn(async move |view, cx| {
            let result = runtime.spawn(async move {
                store.connect(host, auth).await
            }).await;
            let _ = view.update(cx, |view, cx| {
                view.ssh.pending = None;
                match result {
                    Ok(Ok(ConnectOutcome::Connected(_))) => {
                        view.ssh.activity = SshActivity::Connected(id.clone());
                        view.ssh.error = None;
                        // Land the browser on the remote root.
                        let root = view
                            .ssh
                            .hosts
                            .iter()
                            .find(|saved| saved.id == id)
                            .and_then(|saved| saved.initial_path.clone());
                        view.navigate_location(
                            crate::domain::location::Location::Remote {
                                host: id.clone(),
                                path: root.unwrap_or_else(|| PathBuf::from("/")),
                            },
                            cx,
                        );
                    }
                    Ok(Ok(ConnectOutcome::AlreadyConnected)) => {
                        view.ssh.activity = SshActivity::Connected(id.clone());
                    }
                    Ok(Err(message)) => {
                        view.ssh.activity = SshActivity::Idle;
                        view.ssh.error = Some(message);
                    }
                    Err(join) => {
                        view.ssh.activity = SshActivity::Idle;
                        view.ssh.error = Some(format!("SSH task failed: {join}"));
                    }
                }
                cx.notify();
            });
        }));
    }

    /// Disconnect the given host and return the browser home if it was
    /// browsing that remote.
    pub(crate) fn spawn_disconnect(
        &mut self,
        id: HostId,
        cx: &mut Context<crate::app::FileManager>,
    ) {
        let store = self.store.clone();
        let runtime = self.runtime.clone();
        self.pending = Some(cx.spawn(async move |view, cx| {
            runtime.spawn(async move { store.disconnect(&id).await }).await.ok();
            let _ = view.update(cx, |view, cx| {
                view.ssh.pending = None;
                view.ssh.activity = SshActivity::Idle;
                if matches!(
                    view.location,
                    crate::domain::location::Location::Remote { .. }
                ) {
                    view.navigate(view.home.clone(), cx);
                }
                cx.notify();
            });
        }));
    }

    /// Close every session (window close / app quit).
    pub(crate) fn shutdown(&self) {
        let store = self.store.clone();
        let _ = self.runtime.clone().block_on(async move {
            store.disconnect_all().await;
        });
    }
}

#[cfg(test)]
#[path = "../../tests/state/ssh.rs"]
mod tests;
