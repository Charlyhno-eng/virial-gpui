use super::*;
use std::os::unix::fs::symlink;

#[test]
fn fuzzy_scoring_preserves_ascii_and_unicode_character_order() {
    for (path, term, expected) in [
        ("/école/report", "rprt", Some(4)),
        ("/école/report", "éprt", Some(4)),
        ("/école/report", "éé", None),
        ("/école/report", "rté", None),
        ("/àé", "a", None),
        ("/école/report", "report", Some(3)),
    ] {
        assert_eq!(
            score("notes", path, &[term.into()]),
            expected,
            "{path}: {term}"
        );
    }
}

#[test]
fn publishes_partial_project_names_before_deeper_matches_and_lower_priority_roots() {
    let fixture = tempfile::tempdir().unwrap();
    let home = fixture.path().join("home");
    let other = fixture.path().join("other");
    fs::create_dir_all(home.join("cache/dependencies/deep")).unwrap();
    fs::create_dir_all(home.join("projects/jev-codex-pilot")).unwrap();
    fs::create_dir_all(&other).unwrap();
    fs::write(home.join("cache/dependencies/deep/jev-codex"), b"").unwrap();
    fs::write(other.join("jev-codex"), b"").unwrap();

    let mut batches = 0;
    search(
        vec![home.clone(), other],
        "JEV-CODEX",
        true,
        &AtomicBool::new(false),
        |batch| {
            batches += 1;
            assert!(!batch.finished);
            assert_eq!(batch.entries.len(), 1);
            assert_eq!(batch.entries[0].path, home.join("projects/jev-codex-pilot"));
            false
        },
    );
    assert_eq!(batches, 1);
}

#[test]
fn partial_project_name_stays_above_path_matches_when_results_are_limited() {
    let fixture = tempfile::tempdir().unwrap();
    let project = fixture.path().join("jev-codex-pilot");
    fs::create_dir(&project).unwrap();
    for index in 0..150 {
        fs::write(project.join(format!("file-{index:03}")), b"").unwrap();
    }
    fs::write(fixture.path().join("jev-codex"), b"").unwrap();
    let mut results = SearchResults::default();
    search(
        vec![fixture.path().to_path_buf()],
        "jev-codex",
        true,
        &AtomicBool::new(false),
        |batch| {
            results = batch;
            true
        },
    );
    assert!(results.finished);
    assert_eq!(results.entries.len(), RESULT_LIMIT);
    assert_eq!(results.entries[0].name, "jev-codex");
    assert_eq!(results.entries[1].path, project);
}

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
