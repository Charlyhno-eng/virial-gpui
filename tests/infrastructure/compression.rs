use super::*;
use std::io::Read;

#[test]
fn only_zip_and_gzip_are_offered() {
    assert_eq!(
        ArchiveFormat::ALL,
        [ArchiveFormat::Zip, ArchiveFormat::Gzip]
    );
}

#[test]
fn archive_files_are_rejected_without_creating_nested_archives() {
    let root = tempfile::tempdir().unwrap();
    for name in [
        "folder.zip",
        "note.GZ",
        "folder.tar.gz",
        "folder.tgz",
        "old.7z",
        "old.rar",
        "old.tar.xz",
        "old.bz2",
        "old.zst",
    ] {
        let source = root.path().join(name);
        fs::write(&source, b"existing archive").unwrap();
        for format in ArchiveFormat::ALL {
            assert_eq!(
                compress(&source, format).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
            assert!(
                !root
                    .path()
                    .join(format!("{name}{}", format.extension()))
                    .exists()
            );
            assert_eq!(fs::read(&source).unwrap(), b"existing archive");
        }
    }
    // A real directory can have an archive-looking name.
    let folder = root.path().join("directory.zip");
    fs::create_dir(&folder).unwrap();
    compress(&folder, ArchiveFormat::Zip).unwrap();
    assert!(root.path().join("directory.zip.zip").is_file());
}

#[test]
fn gzip_file_round_trip_and_no_overwrite() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("--café.txt");
    let contents = b"hello\0binary\xff";
    fs::write(&source, contents).unwrap();
    compress(&source, ArchiveFormat::Gzip).unwrap();
    let target = destination(&source, ArchiveFormat::Gzip).unwrap();
    assert_eq!(target.file_name().unwrap(), "--café.txt.gz");
    let before = fs::read(&target).unwrap();
    let mut decoded = Vec::new();
    flate2::read::MultiGzDecoder::new(File::open(&target).unwrap())
        .read_to_end(&mut decoded)
        .unwrap();
    assert_eq!(decoded, contents);
    assert!(compress(&source, ArchiveFormat::Gzip).is_err());
    assert_eq!(fs::read(target).unwrap(), before);
}

#[test]
fn zip_file_round_trip_and_failed_compression_cleanup() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("note.txt");
    fs::write(&source, "hello").unwrap();
    compress(&source, ArchiveFormat::Zip).unwrap();
    let mut archive =
        zip::ZipArchive::new(File::open(source.with_file_name("note.txt.zip")).unwrap()).unwrap();
    let mut contents = String::new();
    archive
        .by_name("note.txt")
        .unwrap()
        .read_to_string(&mut contents)
        .unwrap();
    assert_eq!(contents, "hello");
    let missing = root.path().join("missing");
    assert!(compress(&missing, ArchiveFormat::Zip).is_err());
    assert!(!root.path().join("missing.zip").exists());
}

#[test]
fn gzip_folder_creates_readable_tar_gz() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("--folder with spaces");
    fs::create_dir_all(source.join("nested/empty")).unwrap();
    fs::write(source.join("nested/café.txt"), "hello").unwrap();
    compress(&source, ArchiveFormat::Gzip).unwrap();
    let target = destination(&source, ArchiveFormat::Gzip).unwrap();
    assert_eq!(target.file_name().unwrap(), "--folder with spaces.tar.gz");
    // Rename the source to make room for the extraction destination.
    fs::rename(&source, root.path().join("original")).unwrap();
    super::super::extraction::extract(&target).unwrap();
    let restored = source;
    assert_eq!(
        fs::read(restored.join("nested/café.txt")).unwrap(),
        b"hello"
    );
    assert!(restored.join("nested/empty").is_dir());
}

#[test]
fn cancellation_during_streaming_removes_partial_zip_gzip_and_tar_gz() {
    use crate::infrastructure::progress::Progress;
    use std::time::{Duration, Instant};
    let root = tempfile::tempdir().unwrap();
    let folder = root.path().join("folder");
    fs::create_dir(&folder).unwrap();
    let source = folder.join("payload.bin");
    // Incompressible blocks keep the worker active long enough to cancel it.
    let mut seed = 17u64;
    let block: Vec<u8> = (0..128 * 1024)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed as u8
        })
        .collect();
    let mut file = File::create(&source).unwrap();
    for _ in 0..256 {
        file.write_all(&block).unwrap();
    }
    drop(file);
    for (input, format) in [
        (&source, ArchiveFormat::Zip),
        (&source, ArchiveFormat::Gzip),
        (&folder, ArchiveFormat::Gzip),
    ] {
        let target = destination(input, format).unwrap();
        let progress = Progress::default();
        let control = progress.clone();
        let input = input.clone();
        let worker =
            std::thread::spawn(move || compress_with_progress(&input, format, Some(&control)));
        let start = Instant::now();
        while progress.snapshot().bytes_done == 0 && start.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            progress.snapshot().bytes_done > 0,
            "compression did not start"
        );
        progress.cancel();
        assert_eq!(
            worker.join().unwrap().unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        assert!(!target.exists());
        assert_eq!(fs::metadata(&source).unwrap().len(), 32 * 1024 * 1024);
        let mut actual = vec![0; block.len()];
        File::open(&source)
            .unwrap()
            .read_exact(&mut actual)
            .unwrap();
        assert_eq!(actual, block);
    }
}

#[test]
fn cancelled_compression_keeps_existing_archive_and_accepts_a_retry() {
    use crate::infrastructure::progress::Progress;
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("note.txt");
    fs::write(&source, "hello").unwrap();
    for format in ArchiveFormat::ALL {
        let target = destination(&source, format).unwrap();
        fs::write(&target, "existing").unwrap();
        let progress = Progress::default();
        progress.cancel();
        assert_eq!(
            compress_with_progress(&source, format, Some(&progress))
                .unwrap_err()
                .kind(),
            io::ErrorKind::Interrupted
        );
        assert_eq!(fs::read(&target).unwrap(), b"existing");
        fs::remove_file(&target).unwrap();
        compress_with_progress(&source, format, Some(&Progress::default())).unwrap();
        assert!(target.exists());
    }
}

#[test]
#[ignore = "manual compression timing comparison"]
fn fast_compression_benchmark() {
    use std::time::Instant;
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("data.txt");
    let mut file = File::create(&source).unwrap();
    for index in 0..300_000 {
        writeln!(
            file,
            "{index}: {{\"message\":\"a typical document with repeated words\",\"value\":{}}}",
            index * 13
        )
        .unwrap();
    }
    drop(file);
    for format in ArchiveFormat::ALL {
        let baseline = root.path().join("baseline");
        let start = Instant::now();
        if format == ArchiveFormat::Zip {
            let mut writer = zip::ZipWriter::new(File::create(&baseline).unwrap());
            writer
                .start_file(
                    "data.txt",
                    zip::write::SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Deflated),
                )
                .unwrap();
            io::copy(&mut File::open(&source).unwrap(), &mut writer).unwrap();
            writer.finish().unwrap();
        } else {
            let mut writer = flate2::write::GzEncoder::new(
                File::create(&baseline).unwrap(),
                flate2::Compression::default(),
            );
            io::copy(&mut File::open(&source).unwrap(), &mut writer).unwrap();
            writer.finish().unwrap();
        }
        let previous = start.elapsed();
        let start = Instant::now();
        compress(&source, format).unwrap();
        let fast = start.elapsed();
        println!(
            "{format:?}: default {previous:?}, fast {fast:?}, speedup {:.2}x; size {} -> {}",
            previous.as_secs_f64() / fast.as_secs_f64(),
            fs::metadata(baseline).unwrap().len(),
            fs::metadata(destination(&source, format).unwrap())
                .unwrap()
                .len()
        );
    }
}
