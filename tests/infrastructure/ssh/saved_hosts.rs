//! Tests for the JSON persistence of saved SSH hosts.

use super::*;
use std::path::PathBuf;

fn scratch_home(tag: &str) -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("virial-ssh-test-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

fn sample(host: &str, port: u16, user: &str, label: &str) -> HostConfig {
    let mut config = HostConfig::new(host, port, user);
    config.label = label.into();
    config
}

#[test]
fn missing_file_reads_as_empty_list() {
    let home = scratch_home("missing");
    let hosts = SavedHosts::read(&home).unwrap();
    assert!(hosts.is_empty());
}

#[test]
fn upsert_then_read_round_trips_every_field() {
    let home = scratch_home("round-trip");
    let mut host = sample("192.168.40.15", 22, "demon", "Kali");
    host.initial_path = Some(PathBuf::from("/srv/projects"));
    host.auth_hint = AuthHint::Key {
        path: PathBuf::from("/home/demon/.ssh/id_ed25519"),
    };
    SavedHosts::upsert(&home, host).unwrap();

    let hosts = SavedHosts::read(&home).unwrap();
    assert_eq!(hosts.len(), 1);
    assert_eq!(hosts[0].label, "Kali");
    assert_eq!(hosts[0].host, "192.168.40.15");
    assert_eq!(hosts[0].port, 22);
    assert_eq!(hosts[0].username, "demon");
    assert_eq!(hosts[0].initial_path, Some(PathBuf::from("/srv/projects")));
    assert_eq!(
        hosts[0].auth_hint,
        AuthHint::Key {
            path: PathBuf::from("/home/demon/.ssh/id_ed25519")
        }
    );
}

#[test]
fn upsert_replaces_matching_id_in_place() {
    let home = scratch_home("replace");
    SavedHosts::upsert(&home, sample("a.example", 22, "root", "First")).unwrap();
    SavedHosts::upsert(&home, sample("b.example", 22, "root", "Second")).unwrap();
    SavedHosts::upsert(&home, sample("a.example", 22, "root", "Renamed")).unwrap();

    let hosts = SavedHosts::read(&home).unwrap();
    assert_eq!(hosts.len(), 2, "same id must not duplicate");
    assert_eq!(hosts[0].label, "Renamed", "in-place replacement");
    assert_eq!(hosts[1].label, "Second");
}

#[test]
fn interactive_hint_persists_without_any_secret() {
    let home = scratch_home("no-secret");
    let mut host = sample("kali", 22, "demon", "Kali");
    host.auth_hint = AuthHint::Interactive;
    SavedHosts::upsert(&home, host).unwrap();

    let text = std::fs::read_to_string(home.join("virial/ssh_hosts.json")).unwrap();
    assert!(
        !text.contains("secret"),
        "no secret material may reach the disk"
    );
    let hosts = SavedHosts::read(&home).unwrap();
    assert_eq!(hosts[0].auth_hint, AuthHint::Interactive);
}

#[test]
fn remove_drops_only_the_matching_host() {
    let home = scratch_home("remove");
    let first = sample("a.example", 22, "root", "A");
    let second = sample("b.example", 22, "root", "B");
    SavedHosts::upsert(&home, first.clone()).unwrap();
    SavedHosts::upsert(&home, second).unwrap();
    SavedHosts::remove(&home, &first.id).unwrap();

    let hosts = SavedHosts::read(&home).unwrap();
    assert_eq!(hosts.len(), 1);
    assert_eq!(hosts[0].label, "B");

    // Removing again is idempotent.
    SavedHosts::remove(&home, &first.id).unwrap();
    assert_eq!(SavedHosts::read(&home).unwrap().len(), 1);
}

#[test]
fn invalid_json_is_reported_as_invalid_data() {
    let home = scratch_home("invalid");
    let directory = home.join("virial");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("ssh_hosts.json"), "not json at all").unwrap();
    let error = SavedHosts::read(&home).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
}
