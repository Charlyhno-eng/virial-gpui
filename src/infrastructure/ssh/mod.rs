//! SSH remote connections: session lifecycle, SFTP browsing and saved hosts.
//!
//! The module is deliberately split so the transport core compiles and tests
//! without any UI dependency:
//!
//! - [`config`]: saved hosts (`virial/ssh_hosts`) and `~/.ssh/config` parsing.
//! - [`session`]: one live SSH connection (russh) with SFTP operations.
//! - [`store`]: process-wide session registry keyed by [`HostId`].
//!
//! Everything here is UI-agnostic: state machine transitions are plain data
//! ([`SessionEvent`]) that the GPUI layer turns into status-bar badges.

mod config;
mod saved_hosts;
mod session;
mod store;

pub(crate) use config::{AuthHint, HostAuth, HostConfig, HostId};
pub(crate) use saved_hosts::SavedHosts;
pub(crate) use session::{Session, SessionState};
pub(crate) use store::{ConnectOutcome, SshStore};
