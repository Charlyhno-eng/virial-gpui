//! Tests for the GPUI-side SSH manager: pure logic (parsing, auth mapping).

use super::*;

#[test]
fn parse_target_accepts_user_host() {
    let host = SshManager::parse_target("demon@kali").unwrap();
    assert_eq!(host.username, "demon");
    assert_eq!(host.host, "kali");
    assert_eq!(host.port, 22);
    assert_eq!(host.id.to_string(), "demon@kali:22");
}

#[test]
fn parse_target_accepts_user_host_port() {
    let host = SshManager::parse_target("root@example.org:2222").unwrap();
    assert_eq!(host.host, "example.org");
    assert_eq!(host.port, 2222);
    assert_eq!(host.id.to_string(), "root@example.org:2222");
}

#[test]
fn parse_target_rejects_malformed_input() {
    assert!(SshManager::parse_target("").is_err());
    assert!(SshManager::parse_target("no-at-sign").is_err());
    assert!(SshManager::parse_target("@host").is_err());
    assert!(SshManager::parse_target("user@").is_err());
    assert!(SshManager::parse_target("user@host:0").is_err());
    assert!(SshManager::parse_target("user@host:99999").is_err());
    assert!(SshManager::parse_target("user@host:abc").is_err());
}

#[test]
fn parse_target_trims_whitespace() {
    let host = SshManager::parse_target("  demon@kali  ").unwrap();
    assert_eq!(host.host, "kali");
}

#[test]
fn auth_for_maps_hints_without_persisting_credentials() {
    let mut host = HostConfig::new("kali", 22, "demon");

    host.auth_hint = AuthHint::Agent;
    assert!(matches!(
        SshManager::auth_for(&host, Some("typed".into())),
        HostAuth::Agent
    ));

    host.auth_hint = AuthHint::Interactive;
    assert!(matches!(
        SshManager::auth_for(&host, Some("typed".into())),
        HostAuth::Interactive(_)
    ));
    // Nothing typed: fall back to the agent rather than failing loudly.
    assert!(matches!(SshManager::auth_for(&host, None), HostAuth::Agent));

    host.auth_hint = AuthHint::Key {
        path: std::path::PathBuf::from("/id"),
    };
    match SshManager::auth_for(&host, Some("phrase".into())) {
        HostAuth::Key { path, passphrase } => {
            assert_eq!(path, std::path::PathBuf::from("/id"));
            assert_eq!(passphrase.as_deref(), Some("phrase"));
        }
        other => panic!("unexpected auth: {other:?}"),
    }
}

#[test]
fn connected_tracks_the_active_host() {
    let mut manager = SshManager {
        store: Arc::new(SshStore::new()),
        runtime: Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .unwrap(),
        ),
        hosts: Vec::new(),
        activity: SshActivity::Idle,
        error: None,
        pending: None,
        menu_open: false,
        dialog_input: None,
        dialog_auth: SshAuthMode::default(),
        dialog_aux: None,
    };
    let id = HostId::new("kali", 22, "demon");
    assert!(!manager.connected(&id));
    manager.activity = SshActivity::Connected(id.clone());
    assert!(manager.connected(&id));
    manager.activity = SshActivity::Connecting(id.to_string());
    assert!(!manager.connected(&id));
}

#[test]
fn parse_target_keeps_dotted_hyphenated_and_underscored_hostnames() {
    let named = SshManager::parse_target("deploy@build.internal:2200").unwrap();
    assert_eq!(named.host, "build.internal");
    assert_eq!(named.port, 2200);
    assert_eq!(named.username, "deploy");
    // Sub-domains, hyphens and underscores survive the split.
    let hyphen = SshManager::parse_target("ci@build-eu.example.org").unwrap();
    assert_eq!(hyphen.host, "build-eu.example.org");
    assert_eq!(hyphen.port, 22);
    let underscore = SshManager::parse_target("deploy@build_eu.internal").unwrap();
    assert_eq!(underscore.host, "build_eu.internal");
}

#[test]
fn parse_target_rejects_empty_port_suffix() {
    assert!(SshManager::parse_target("user@host:").is_err());
}

#[test]
fn connected_matches_only_the_exact_id() {
    let mut manager = SshManager {
        store: Arc::new(SshStore::new()),
        runtime: Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .unwrap(),
        ),
        hosts: Vec::new(),
        activity: SshActivity::Idle,
        error: None,
        pending: None,
        menu_open: false,
        dialog_input: None,
        dialog_auth: SshAuthMode::default(),
        dialog_aux: None,
    };
    let kali = HostId::new("kali", 22, "demon");
    let other = HostId::new("nas", 22, "demon");
    manager.activity = SshActivity::Connected(kali.clone());
    assert!(manager.connected(&kali));
    assert!(
        !manager.connected(&other),
        "a different host is not connected"
    );
}
