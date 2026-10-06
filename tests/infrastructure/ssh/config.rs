//! Tests for `infrastructure::ssh::config`.

use super::*;

#[test]
fn host_ids_are_user_host_port_keys() {
    let id = HostId::new("192.168.40.15", 22, "demon");
    assert_eq!(id.to_string(), "demon@192.168.40.15:22");
    let custom = HostId::new("example.org", 2222, "root");
    assert_eq!(custom.to_string(), "root@example.org:2222");
}

#[test]
fn host_ids_are_unique_per_user_and_port() {
    let first = HostId::new("example.org", 22, "root");
    let second = HostId::new("example.org", 2222, "root");
    let third = HostId::new("example.org", 22, "admin");
    assert_ne!(first, second);
    assert_ne!(first, third);
}

#[test]
fn new_configs_default_to_agent_and_host_label() {
    let host = HostConfig::new("kali", 22, "demon");
    assert_eq!(host.id.to_string(), "demon@kali:22");
    assert_eq!(host.label, "kali");
    assert_eq!(host.auth_hint, AuthHint::Agent);
    assert_eq!(host.initial_path, None);
}

#[test]
fn display_prefaces_label_with_identifier() {
    let mut host = HostConfig::new("192.168.40.15", 22, "demon");
    host.label = "Kali build box".into();
    assert_eq!(host.display(), "Kali build box (demon@192.168.40.15:22)");
}

#[test]
fn auth_labels_are_stable() {
    assert_eq!(HostAuth::Agent.label(), "SSH agent");
    assert_eq!(
        HostAuth::Key {
            path: PathBuf::from("/home/user/.ssh/id_ed25519"),
            passphrase: None,
        }
        .label(),
        "Private key"
    );
    assert_eq!(HostAuth::Interactive("secret".into()).label(), "Password");
}
