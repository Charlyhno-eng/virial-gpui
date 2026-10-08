use super::*;
#[test]
fn merges_desktop_and_own_history_decodes_urls_and_skips_missing_files() {
    let root = std::env::temp_dir().join(format!("virial-recent-test-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let old = root.join("old & space.txt");
    let new = root.join("new.txt");
    fs::write(&old, "old").unwrap();
    fs::write(&new, "new").unwrap();
    let uri = Url::from_file_path(&old)
        .unwrap()
        .to_string()
        .replace('&', "&amp;");
    fs::write(root.join("recently-used.xbel"), format!(r#"<xbel><bookmark href="{uri}" modified="2026-01-01T02:00:00+02:00"/><bookmark href="https://example.com/"/><bookmark href="file:///nonexistent/virial-test"/></xbel>"#)).unwrap();
    record(&root, &new).unwrap();
    record(&root, &new).unwrap();
    let entries = read(&root, false).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].path, new);
    assert_eq!(entries[1].path, old);
    // file:///a is only a valid file URL on single-root platforms.
    #[cfg(unix)]
    assert_eq!(desktop_records(r#"<xbel><bookmark href="file:///a" visited="2026-01-01T01:00:00+01:00"/><bookmark href="file:///b" visited="2026-01-01T00:00:00Z"/></xbel>"#).unwrap()[0].0,
        desktop_records(r#"<xbel><bookmark href="file:///b" visited="2026-01-01T00:00:00Z"/></xbel>"#).unwrap()[0].0);
    assert!(desktop_records("<broken>").is_err());
    assert!(read(&root.join("absent"), false).unwrap().is_empty());
    fs::remove_dir_all(root).unwrap();
}
