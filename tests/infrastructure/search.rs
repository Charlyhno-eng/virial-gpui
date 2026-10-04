use super::*;
use std::os::unix::fs::symlink;

#[test]
fn ranks_names_and_matches_case_insensitive_path_terms_and_fuzzy_letters() {
    let terms = vec!["report".into()];
    assert!(
        score("report", "/a/report", &terms) < score("report-final", "/a/report-final", &terms)
    );
    assert!(
        score("report-final", "/a/report-final", &terms)
            < score("my-report", "/a/my-report", &terms)
    );
    assert!(score("notes", "/reports/notes", &terms).is_some());
    assert!(score("rpt", "/reports/rpt", &["rprts".into()]).is_some());
    assert!(score("notes", "/docs/notes", &terms).is_none());
    assert!(score("report", "/work/report", &["work".into(), "report".into()]).is_some());
    assert!(score("report", "/work/report", &["work".into(), "absent".into()]).is_none());
}

#[test]
fn searches_unvisited_nested_files_with_hidden_policy_and_without_symlink_cycles() {
    let root = std::env::temp_dir().join(format!("virial-global-search-{}", std::process::id()));
    fs::create_dir_all(root.join("Elsewhere/Deep")).unwrap();
    fs::create_dir(root.join(".private")).unwrap();
    fs::write(root.join("Elsewhere/Deep/Report.txt"), b"hello").unwrap();
    fs::write(root.join(".private/Report-secret"), b"").unwrap();
    symlink(&root, root.join("Elsewhere/loop")).unwrap();
    symlink(root.join("Elsewhere/Deep"), root.join("Report-link")).unwrap();
    let mut results = SearchResults::default();
    search(
        vec![root.clone(), root.join("Elsewhere")],
        "REPORT",
        false,
        &AtomicBool::new(false),
        |batch| {
            results = batch;
            true
        },
    );
    assert!(results.finished);
    assert_eq!(results.entries.len(), 2);
    assert!(
        results
            .entries
            .iter()
            .any(|entry| entry.path == root.join("Elsewhere/Deep/Report.txt") && !entry.directory)
    );
    assert!(
        results
            .entries
            .iter()
            .any(|entry| entry.name == "Report-link" && entry.directory)
    );
    search(
        vec![root.clone()],
        "private report",
        true,
        &AtomicBool::new(false),
        |batch| {
            results = batch;
            true
        },
    );
    assert_eq!(results.entries.len(), 1);
    assert_eq!(results.entries[0].name, "Report-secret");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn bounds_results_and_handles_cancellation_empty_queries_and_missing_roots() {
    let root = std::env::temp_dir().join(format!("virial-global-limit-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    for index in 0..150 {
        fs::write(root.join(format!("match-{index:03}")), b"").unwrap();
    }
    let mut results = SearchResults::default();
    search(
        vec![root.clone()],
        "match",
        false,
        &AtomicBool::new(false),
        |batch| {
            results = batch;
            true
        },
    );
    assert_eq!(results.entries.len(), RESULT_LIMIT);
    assert_eq!(results.entries[0].name, "match-000");
    assert_eq!(results.entries[99].name, "match-099");
    search(
        vec![root.join("missing")],
        "match",
        false,
        &AtomicBool::new(false),
        |batch| {
            results = batch;
            true
        },
    );
    assert!(results.finished);
    assert_eq!(results.skipped, 1);
    assert!(results.entries.is_empty());
    for (query, cancelled) in [("match", true), ("  ", false)] {
        search(
            vec![root.clone()],
            query,
            false,
            &AtomicBool::new(cancelled),
            |_| panic!("No results should be published"),
        );
    }
    let cancelled = AtomicBool::new(false);
    search(vec![root.clone()], "match", false, &cancelled, |_| {
        cancelled.store(true, Ordering::Relaxed);
        false
    });
    fs::remove_dir_all(root).unwrap();
}
