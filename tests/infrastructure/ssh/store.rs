//! Tests for the session registry. Pure state-machine coverage: no network.

use super::*;

fn config(tag: &str) -> HostConfig {
    HostConfig::new(&format!("{tag}.example"), 22, "root")
}

#[tokio::test]
async fn empty_store_has_no_sessions() {
    let store = SshStore::new();
    assert!(store.connected_ids().await.is_empty());
    assert!(store.session(&config("a").id).await.is_none());
}

#[tokio::test]
async fn disconnect_of_unknown_id_is_a_no_op() {
    let store = SshStore::new();
    store.disconnect(&config("ghost").id).await;
    store.disconnect_all().await;
    assert!(store.connected_ids().await.is_empty());
}

#[tokio::test]
async fn connect_failure_keeps_the_store_empty() {
    // RFC 5737 / RFC 3849 documentation addresses: connect fails fast or times
    // out, either way no session may be registered.
    let store = SshStore::new();
    let host = HostConfig::new("192.0.2.1", 2222, "nobody");
    let result = store
        .connect(host.clone(), HostAuth::Interactive("unused".into()))
        .await;
    assert!(result.is_err(), "unreachable host must fail: {result:?}");
    assert!(store.session(&host.id).await.is_none());
    assert!(store.connected_ids().await.is_empty());
}

/// Real connect paths need a live SSH server; they are covered by the
/// integration cycle on the build box (see tests/infrastructure/ssh/README).
#[tokio::test]
async fn disconnect_all_drains_every_session() {
    let store = SshStore::new();
    store.disconnect_all().await;
    assert!(store.connected_ids().await.is_empty());
}
