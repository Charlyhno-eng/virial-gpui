//! Process-wide registry of live SSH sessions.
//!
//! The UI layer never owns sessions directly: it asks the store to connect,
//! lists, or disconnects, and reads back plain [`SessionState`] values. This
//! keeps GPUI entities free of transport types and gives one obvious place to
//! enforce "one session per host id".

use super::config::{HostAuth, HostConfig, HostId};
use super::session::{Session, SessionState};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex as AsyncMutex;

/// Result of a connect attempt reported back to the UI thread.
#[derive(Clone)]
pub(crate) enum ConnectOutcome {
    /// A previously connected session for the same id is still alive.
    AlreadyConnected,
    /// The session is now connected and browsable.
    Connected(Arc<Session>),
}

impl std::fmt::Debug for ConnectOutcome {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyConnected => formatter.write_str("AlreadyConnected"),
            // Session internals are opaque; the variant alone is meaningful.
            Self::Connected(_) => formatter.write_str("Connected(..)"),
        }
    }
}

/// Shared registry backed by a tokio mutex (all calls are async anyway).
#[derive(Default)]
pub(crate) struct SshStore {
    sessions: AsyncMutex<HashMap<HostId, Arc<Session>>>,
}

impl SshStore {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// List a remote directory with the live session of `id`. russh futures
    /// need a tokio context, so the caller passes its runtime handle; when
    /// `runtime` is `None` the caller is already inside one.
    pub(crate) async fn browse(
        self: &Arc<Self>,
        id: &HostId,
        directory: &std::path::Path,
        runtime: Option<tokio::runtime::Handle>,
    ) -> Result<Vec<crate::domain::models::Entry>, String> {
        let store = self.clone();
        let id = id.clone();
        let directory = directory.to_path_buf();
        let work = async move {
            let Some(session) = store.session(&id).await else {
                return Err("SSH session is not connected".into());
            };
            session.read_dir(&directory).await
        };
        match runtime {
            Some(handle) => handle
                .spawn(work)
                .await
                .unwrap_or_else(|join| Err(format!("SSH task failed: {join}"))),
            None => work.await,
        }
    }

    /// Connect (or reuse) the session for `config`.
    pub(crate) async fn connect(
        &self,
        config: HostConfig,
        auth: HostAuth,
    ) -> Result<ConnectOutcome, String> {
        let mut sessions = self.sessions.lock().await;
        if let Some(existing) = sessions.get(&config.id) {
            if matches!(existing.state(), SessionState::Connected) {
                return Ok(ConnectOutcome::AlreadyConnected);
            }
        }
        let session = Session::connect(config.clone(), auth).await?;
        let session = Arc::new(session);
        sessions.insert(config.id, session.clone());
        Ok(ConnectOutcome::Connected(session))
    }

    /// The live session for `id`, if connected.
    pub(crate) async fn session(&self, id: &HostId) -> Option<Arc<Session>> {
        self.sessions
            .lock()
            .await
            .get(id)
            .filter(|session| matches!(session.state(), SessionState::Connected))
            .cloned()
    }

    /// Disconnect and forget the session for `id`.
    pub(crate) async fn disconnect(&self, id: &HostId) {
        let removed = self.sessions.lock().await.remove(id);
        if let Some(session) = removed {
            session.disconnect().await;
        }
    }

    /// Ids of every live session, sorted for stable UI ordering.
    pub(crate) async fn connected_ids(&self) -> Vec<HostId> {
        let sessions = self.sessions.lock().await;
        let mut ids: Vec<HostId> = sessions
            .iter()
            .filter(|(_, session)| matches!(session.state(), SessionState::Connected))
            .map(|(id, _)| id.clone())
            .collect();
        ids.sort();
        ids
    }

    /// Disconnect everything (window close / application quit).
    pub(crate) async fn disconnect_all(&self) {
        let mut sessions = self.sessions.lock().await;
        for (_, session) in sessions.drain() {
            session.disconnect().await;
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/infrastructure/ssh/store.rs"]
mod tests;
