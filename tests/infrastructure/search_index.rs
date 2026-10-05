use super::*;
use std::os::unix::fs::symlink;

fn inventory(root: &Path) -> Inventory {
    Inventory::new(SearchHandle::default(), vec![root.to_path_buf()], None)
}

fn finish(inventory: &mut Inventory) {
    while let Some(path) = inventory.pending.pop_front() {
        inventory.scan_directory(path, &AtomicBool::new(false));
    }
    inventory.finish_scan();
}

fn query(inventory: &Inventory, query: &str, hidden: bool) -> SearchResults {
    inventory
        .handle
        .query(query, hidden, &AtomicBool::new(false))
        .unwrap()
}

#[test]
fn indexed_results_preserve_scan_ranking_unicode_hidden_paths_and_symlinks() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path();
    fs::create_dir_all(root.join("Reports/Deep")).unwrap();
    fs::create_dir_all(root.join(".secret/visible")).unwrap();
    for name in [
        "report",
        "Report-final",
        "my-report",
        "Été.txt",
        "report-été",
        "report-work",
    ] {
        fs::write(root.join(name), b"").unwrap();
    }
    fs::write(root.join("Reports/Deep/notes"), b"").unwrap();
    fs::write(root.join(".secret/visible/report"), b"").unwrap();
    symlink(root, root.join("Reports/loop")).unwrap();
    symlink(root.join("Reports"), root.join("Report-link")).unwrap();
    let mut index = inventory(root);
    index.begin_scan(false);
    finish(&mut index);
    for text in [
        "REPORT",
        "report work",
        "ÉTÉ",
        "rprts",
        "secret report",
        "absent",
        "report-report",
    ] {
        for hidden in [false, true] {
            let mut expected = SearchResults::default();
            crate::infrastructure::search::search(
                vec![root.to_path_buf()],
                text,
                hidden,
                &AtomicBool::new(false),
                |batch| {
                    expected = batch;
                    true
                },
            );
            let actual = query(&index, text, hidden);
            let paths = |results: &SearchResults| {
                results
                    .entries
                    .iter()
                    .map(|entry| (entry.path.clone(), entry.directory))
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                paths(&actual),
                paths(&expected),
                "query={text} hidden={hidden}"
            );
            assert!(actual.finished);
        }
    }
}

#[test]
fn prefix_fast_path_still_selects_global_best_hundred() {
    let fixture = tempfile::tempdir().unwrap();
    for directory in ["z", "a"] {
        fs::create_dir(fixture.path().join(directory)).unwrap();
        for file in 0..150 {
            fs::write(
                fixture
                    .path()
                    .join(directory)
                    .join(format!("report-{file:03}")),
                b"",
            )
            .unwrap();
        }
    }
    fs::write(fixture.path().join("z/report"), b"").unwrap();
    let mut index = inventory(fixture.path());
    index.begin_scan(false);
    finish(&mut index);
    let actual = query(&index, "report", true);
    assert_eq!(actual.entries.len(), RESULT_LIMIT);
    assert_eq!(actual.entries[0].path, fixture.path().join("z/report"));
    assert_eq!(actual.entries[1].path, fixture.path().join("a/report-000"));
    assert_eq!(actual.entries[99].path, fixture.path().join("a/report-098"));
    // The warm catalog performs no traversal: it can still answer after disk removal.
    fs::remove_dir_all(fixture.path().join("a")).unwrap();
    assert_eq!(query(&index, "report", true).entries.len(), RESULT_LIMIT);
    index.begin_scan(false);
    finish(&mut index);
    assert!(
        query(&index, "report", true)
            .entries
            .iter()
            .all(|entry| entry.path.starts_with(fixture.path().join("z")))
    );
}

#[test]
fn notifications_refresh_creations_renames_and_removed_subtrees() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path();
    fs::create_dir_all(root.join("original/deep")).unwrap();
    fs::write(root.join("original/deep/report"), b"").unwrap();
    let mut index = inventory(root);
    assert!(index.watches.fd.is_some(), "inotify unavailable");
    index.begin_scan(false);
    finish(&mut index);
    fs::rename(root.join("original"), root.join("renamed")).unwrap();
    fs::create_dir_all(root.join("new/deep")).unwrap();
    fs::write(root.join("new/deep/report-new"), b"").unwrap();
    let (changed, overflow) = index.watches.changes();
    assert!(!overflow);
    assert!(changed.contains(root));
    index.queue_changes(changed, overflow);
    finish(&mut index);
    let actual = query(&index, "report", true);
    assert_eq!(actual.entries.len(), 2);
    assert!(
        actual
            .entries
            .iter()
            .any(|entry| entry.path == root.join("renamed/deep/report"))
    );
    assert!(
        actual
            .entries
            .iter()
            .any(|entry| entry.path == root.join("new/deep/report-new"))
    );
    fs::remove_dir_all(root.join("renamed")).unwrap();
    fs::remove_file(root.join("new/deep/report-new")).unwrap();
    let (changed, overflow) = index.watches.changes();
    index.queue_changes(changed, overflow);
    finish(&mut index);
    let results = query(&index, "report", true);
    assert!(results.entries.is_empty());
    assert_eq!(results.skipped, 0);
}

#[test]
fn persisted_catalog_reuses_unchanged_snapshots_and_reconciles_stale_paths() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("files");
    fs::create_dir_all(root.join("gone/deep")).unwrap();
    fs::write(root.join("gone/deep/report-old"), b"").unwrap();
    fs::write(root.join("report-kept"), b"").unwrap();
    let cache = fixture.path().join("cache/catalog");
    let mut index = Inventory::new(
        SearchHandle::default(),
        vec![root.clone()],
        Some(cache.clone()),
    );
    index.begin_scan(false);
    finish(&mut index);
    index.save().unwrap();
    let mut restored = Inventory::new(SearchHandle::default(), vec![root.clone()], Some(cache));
    restored.load(&AtomicBool::new(false)).unwrap();
    assert_eq!(query(&restored, "report", true).entries.len(), 2);
    let before = restored.handle.0.read().unwrap().directories[&root].clone();
    restored.begin_scan(false);
    finish(&mut restored);
    assert_eq!(restored.seen.capacity(), 0);
    assert_eq!(restored.visited.capacity(), 0);
    assert_eq!(restored.pending.capacity(), 0);
    assert!(Arc::ptr_eq(
        &before,
        &restored.handle.0.read().unwrap().directories[&root]
    ));
    fs::remove_dir_all(root.join("gone")).unwrap();
    fs::write(root.join("report-new"), b"").unwrap();
    restored.begin_scan(true);
    finish(&mut restored);
    let actual = query(&restored, "report", true);
    assert_eq!(actual.entries.len(), 2);
    assert!(
        actual
            .entries
            .iter()
            .all(|entry| entry.name != "report-old")
    );
}

#[test]
fn cache_preserves_non_utf8_names_and_rejects_other_roots() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("files");
    fs::create_dir(&root).unwrap();
    let name = OsString::from_vec(b"report-\xff".to_vec());
    fs::write(root.join(&name), b"").unwrap();
    let cache = fixture.path().join("cache/catalog");
    let mut index = Inventory::new(
        SearchHandle::default(),
        vec![root.clone()],
        Some(cache.clone()),
    );
    index.begin_scan(false);
    finish(&mut index);
    index.save().unwrap();
    let mut restored = Inventory::new(
        SearchHandle::default(),
        vec![root.clone()],
        Some(cache.clone()),
    );
    restored.load(&AtomicBool::new(false)).unwrap();
    assert_eq!(
        query(&restored, "report", true).entries[0].path,
        root.join(name)
    );
    let mut other = Inventory::new(
        SearchHandle::default(),
        vec![fixture.path().to_path_buf()],
        Some(cache),
    );
    assert!(other.load(&AtomicBool::new(false)).is_err());
    assert!(other.handle.0.read().unwrap().directories.is_empty());
}

#[test]
fn empty_queries_and_cancellation_do_not_publish_results() {
    let handle = SearchHandle::default();
    assert!(handle.query("  ", true, &AtomicBool::new(false)).is_none());
    assert!(
        handle
            .query("report", true, &AtomicBool::new(true))
            .is_none()
    );
}

#[test]
fn background_service_streams_inventory_and_live_changes() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("files");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("report-original"), b"").unwrap();
    let cache = fixture.path().join("cache/catalog");
    // A damaged cache must fall back to an inventory, without external commands.
    fs::create_dir_all(cache.parent().unwrap()).unwrap();
    fs::write(&cache, b"damaged cache").unwrap();
    let index = SearchIndex::start(vec![root.clone()], cache.clone());
    let handle = index.handle();
    let wait_for = |condition: &dyn Fn(&SearchResults) -> bool| {
        let started = Instant::now();
        loop {
            let results = handle
                .query("report", true, &AtomicBool::new(false))
                .unwrap();
            if condition(&results) {
                return;
            }
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "background index did not update"
            );
            thread::sleep(Duration::from_millis(10));
        }
    };
    wait_for(&|results| results.finished && results.entries.len() == 1);
    fs::create_dir_all(root.join("new/deep")).unwrap();
    fs::write(root.join("new/deep/report-added"), b"").unwrap();
    wait_for(&|results| results.entries.len() == 2);
    fs::remove_file(root.join("report-original")).unwrap();
    wait_for(&|results| results.entries.len() == 1 && results.entries[0].name == "report-added");
    drop(index);
}

#[test]
fn notification_overflow_requests_a_complete_reconciliation() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path();
    fs::write(root.join("report-old"), b"").unwrap();
    let mut index = inventory(root);
    index.begin_scan(false);
    finish(&mut index);
    fs::remove_file(root.join("report-old")).unwrap();
    fs::write(root.join("report-new"), b"").unwrap();
    // Simulate a lost inotify batch: no changed paths are available.
    index.queue_changes(HashSet::new(), true);
    assert!(index.full_scan && index.force_scan);
    finish(&mut index);
    assert_eq!(query(&index, "report", true).entries[0].name, "report-new");
    index.begin_scan(false);
    index.queue_changes(HashSet::new(), true);
    assert!(
        index.rescan,
        "overflow during inventory must trigger a second pass"
    );
}

#[test]
#[ignore = "run explicitly in release mode to measure indexed search performance"]
fn indexed_search_performance() {
    assert!(!cfg!(debug_assertions), "Run with --release");
    let resident_kib = || {
        fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|status| {
                status
                    .lines()
                    .find(|line| line.starts_with("VmRSS:"))
                    .and_then(|line| line.split_whitespace().nth(1)?.parse::<usize>().ok())
            })
    };
    let before = resident_kib();
    let root = PathBuf::from("/virial-benchmark");
    let index = inventory(&root);
    // Synthetic catalog isolates query CPU/memory cost from fixture creation and I/O.
    for directory in 0..1000 {
        let path = root.join(format!("folder-{directory:04}"));
        let items = (0..1000)
            .map(|file| StoredItem {
                name: format!("report-{file:04}.txt")
                    .into_bytes()
                    .into_boxed_slice(),
                kind: 0,
            })
            .collect();
        let snapshot = Arc::new(Directory::new(
            StoredDirectory {
                path: path.as_os_str().as_bytes().to_vec(),
                stamp: Stamp(0, 0, 0, 0, 0, 0),
                items,
            },
            std::slice::from_ref(&root),
        ));
        index
            .handle
            .0
            .write()
            .unwrap()
            .directories
            .insert(path, snapshot);
    }
    index.handle.0.write().unwrap().finished = true;
    if let (Some(before), Some(after)) = (before, resident_kib()) {
        println!(
            "PERF indexed_1000000_paths resident_delta_kib={}",
            after.saturating_sub(before)
        );
    }
    for text in [
        "report",
        "report-0999",
        "rprt",
        "folder-0999 report-0999",
        "absent",
    ] {
        query(&index, text, true);
        let mut samples = Vec::new();
        for _ in 0..7 {
            let start = Instant::now();
            let results = query(&index, text, true);
            assert!(results.finished);
            assert!(results.entries.len() <= RESULT_LIMIT);
            if text == "absent" {
                assert!(results.entries.is_empty());
            }
            std::hint::black_box(results);
            samples.push(start.elapsed());
        }
        samples.sort_unstable();
        println!(
            "PERF indexed_1000000_paths query={text:?}: median_ms={:.3} max_ms={:.3}",
            samples[3].as_secs_f64() * 1000.,
            samples[6].as_secs_f64() * 1000.
        );
        assert!(
            samples[3] < Duration::from_secs(2),
            "indexed search exceeded smoke budget"
        );
    }
}

#[test]
fn compact_catalog_shares_long_parent_paths_and_retains_original_names() {
    let path = PathBuf::from(format!("/{}", "parent/".repeat(80)));
    let snapshot = Directory::new(
        StoredDirectory {
            path: path.as_os_str().as_bytes().to_vec(),
            stamp: Stamp(0, 0, 0, 0, 0, 0),
            items: (0..1000)
                .map(|file| StoredItem {
                    name: format!("Report-{file:04}.txt")
                        .into_bytes()
                        .into_boxed_slice(),
                    kind: 0,
                })
                .collect(),
        },
        &[PathBuf::from("/")],
    );
    let allocated = std::mem::size_of_val(snapshot.items.as_ref())
        + snapshot
            .items
            .iter()
            .map(|item| item.name_key.len())
            .sum::<usize>()
        + std::mem::size_of_val(snapshot.stored.items.as_ref())
        + snapshot
            .stored
            .items
            .iter()
            .map(|item| item.name.len())
            .sum::<usize>();
    // Per-file payload must stay independent of a 560-byte parent path.
    assert!(allocated < 100 * 1024, "catalog payload: {allocated}");
    assert_eq!(
        snapshot.item_path(&snapshot.items[0]),
        path.join("Report-0000.txt")
    );
}

#[test]
fn compact_catalog_keeps_existing_cache_format_and_non_utf8_names() {
    // This is the Vec-based cache format written before compact storage.
    let old = r#"{"path":[47,116,101,115,116],"stamp":[0,0,0,0,0,0],"items":[{"name":[255,46,116,120,116],"kind":0}]}"#;
    let stored: StoredDirectory = serde_json::from_str(old).unwrap();
    assert_eq!(serde_json::to_string(&stored).unwrap(), old);
    let directory = Directory::new(stored, &[PathBuf::from("/")]);
    assert_eq!(
        directory
            .item_path(&directory.items[0])
            .as_os_str()
            .as_bytes(),
        b"/test/\xff.txt"
    );
}

#[test]
fn notifications_invalidate_snapshots_even_when_directory_stamp_matches() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path();
    fs::write(root.join("report-old"), b"").unwrap();
    let mut index = inventory(root);
    index.begin_scan(false);
    finish(&mut index);
    fs::rename(root.join("report-old"), root.join("report-new")).unwrap();
    // Simulate a coarse filesystem timestamp hiding an enumeration change.
    let mut catalog = index.handle.0.write().unwrap();
    Arc::get_mut(catalog.directories.get_mut(root).unwrap())
        .unwrap()
        .stored
        .stamp = Stamp::read(&fs::metadata(root).unwrap());
    drop(catalog);
    let (changed, overflow) = index.watches.changes();
    assert!(changed.contains(root));
    index.queue_changes(changed, overflow);
    finish(&mut index);
    assert_eq!(query(&index, "report", true).entries[0].name, "report-new");
}

#[test]
fn joined_path_order_matches_full_keys_across_directory_boundaries() {
    let parents = ["", "/", "/a/", "/a/b/", "/a-b/", "/étÉ/", "/report/"];
    let names = ["", "report", "report-more", "a", "b", "Été", "é", "🔥"];
    for left_parent in parents {
        for left_name in names {
            for right_parent in parents {
                for right_name in names {
                    assert_eq!(
                        joined_key_cmp(
                            left_parent.as_bytes(),
                            left_name.as_bytes(),
                            right_parent.as_bytes(),
                            right_name.as_bytes(),
                        ),
                        format!("{left_parent}{left_name}")
                            .cmp(&format!("{right_parent}{right_name}"))
                    );
                }
            }
        }
    }
}
