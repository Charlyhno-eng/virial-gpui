use super::{search, storage};
use std::{fs, sync::atomic::AtomicBool, time::Instant};

// A broad smoke budget catches severe slowdowns without treating shared CI
// runners as dedicated benchmark machines. Fixture creation is not timed.
fn measure(name: &str, mut operation: impl FnMut()) {
    operation(); // Warm the filesystem cache before collecting seven samples.
    let mut samples = Vec::new();
    for _ in 0..7 {
        let started = Instant::now();
        operation();
        samples.push(started.elapsed());
    }
    samples.sort_unstable();
    println!(
        "PERF {name}: median_ms={:.3} max_ms={:.3} samples=7 budget_ms=2000",
        samples[3].as_secs_f64() * 1000.0,
        samples[6].as_secs_f64() * 1000.0,
    );
    assert!(
        samples[3].as_secs_f64() < 2.0,
        "{name} exceeded the two-second median smoke budget"
    );
}

#[test]
#[ignore = "run explicitly in release mode to measure filesystem performance"]
fn filesystem_performance() {
    assert!(!cfg!(debug_assertions), "Run this test with --release");
    let fixture = tempfile::tempdir().unwrap();
    let listing = fixture.path().join("listing");
    let tree = fixture.path().join("tree");
    fs::create_dir(&listing).unwrap();
    fs::create_dir(&tree).unwrap();
    for directory in 0..50 {
        let branch = tree.join(format!("folder-{directory:02}"));
        fs::create_dir(&branch).unwrap();
        for file in 0..100 {
            let index = directory * 100 + file;
            let name = format!("report-{index:04}.txt");
            fs::write(listing.join(&name), b"fixture").unwrap();
            fs::write(branch.join(name), b"fixture").unwrap();
        }
    }

    measure("list_5000_files", || {
        let entries = storage::read_directory(&listing, false).unwrap();
        assert_eq!(entries.len(), 5000);
        assert_eq!(entries[0].name, "report-0000.txt");
        assert_eq!(entries[4999].name, "report-4999.txt");
        std::hint::black_box(entries);
    });
    measure("search_5000_files_in_50_folders", || {
        let mut results = search::SearchResults::default();
        search::search(
            vec![tree.clone()],
            "report",
            false,
            &AtomicBool::new(false),
            |batch| {
                results = batch;
                true
            },
        );
        assert!(results.finished);
        assert_eq!(results.skipped, 0);
        assert_eq!(results.entries.len(), search::RESULT_LIMIT);
        assert_eq!(results.entries[0].name, "report-0000.txt");
        std::hint::black_box(results);
    });
}

// Informational comparison on the host filesystem; storage capabilities vary,
// so timings are reported without a hardware-dependent pass/fail threshold.
#[test]
#[ignore = "run explicitly in release mode to measure transfer performance"]
fn transfer_performance() {
    use super::{operations, progress::Progress, undo};
    use std::{io::Write, time::Duration};
    assert!(!cfg!(debug_assertions), "Run this test with --release");
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("source");
    let destination = fixture.path().join("destination");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&destination).unwrap();
    let mut large = fs::File::create(source.join("large.bin")).unwrap();
    let block = vec![0x5a; 1024 * 1024];
    for _ in 0..64 {
        large.write_all(&block).unwrap();
    }
    large.sync_all().unwrap();
    for index in 0..1000 {
        fs::write(source.join(format!("small-{index}")), [0x3c; 4096]).unwrap();
    }
    let measure = |name: &str, operation: &mut dyn FnMut() -> Duration| {
        operation();
        let mut samples = (0..7).map(|_| operation()).collect::<Vec<_>>();
        samples.sort_unstable();
        println!(
            "PERF {name}: median_ms={:.3} max_ms={:.3} samples=7",
            samples[3].as_secs_f64() * 1000.,
            samples[6].as_secs_f64() * 1000.
        );
    };
    measure("copy_64mib_and_1000_files_with_progress", &mut || {
        let target = destination.join("copy");
        let started = Instant::now();
        operations::copy_with_progress(&source, &target, Some(&Progress::default())).unwrap();
        let elapsed = started.elapsed();
        assert_eq!(
            fs::metadata(target.join("large.bin")).unwrap().len(),
            64 * 1024 * 1024
        );
        assert_eq!(fs::read_dir(&target).unwrap().count(), 1001);
        fs::remove_dir_all(&target).unwrap();
        elapsed
    });
    measure("move_64mib_and_1000_files_with_undo", &mut || {
        let data = tempfile::tempdir_in(fixture.path()).unwrap();
        let started = Instant::now();
        undo::execute_with_progress(
            data.path(),
            operations::Operation::Transfer {
                sources: vec![source.clone()],
                directory: destination.clone(),
                cut: true,
            },
            Some(&Progress::default()),
        )
        .unwrap();
        let elapsed = started.elapsed();
        assert!(!source.exists());
        fs::rename(destination.join("source"), &source).unwrap();
        elapsed
    });
}
