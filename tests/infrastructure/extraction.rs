use super::*;
use crate::infrastructure::{
    compression::{self, ArchiveFormat},
    operations::{self, Operation},
};
use std::io::Write;

fn zip(path: &Path, name: &str, link: bool) {
    let mut writer = zip::ZipWriter::new(File::create(path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    if link {
        writer.add_symlink(name, "../outside", options).unwrap();
    } else {
        writer.start_file(name, options).unwrap();
        writer.write_all(b"hello").unwrap();
    }
    writer.finish().unwrap();
}

#[test]
fn extracts_zip_implicit_directories_and_empty_folders() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("received.ZIP");
    zip(&path, "nested/café.txt", false);
    // Exercise the operation used by the UI.
    operations::execute(Operation::Extract(path.clone())).unwrap();
    assert_eq!(
        fs::read(root.path().join("received/nested/café.txt")).unwrap(),
        b"hello"
    );
    assert!(path.is_file());
    let source = root.path().join("folder");
    fs::create_dir_all(source.join("empty")).unwrap();
    compression::compress(&source, ArchiveFormat::Zip).unwrap();
    fs::remove_dir_all(&source).unwrap();
    extract(&root.path().join("folder.zip")).unwrap();
    assert!(source.join("empty").is_dir());
}

#[test]
fn gzip_extraction_refuses_existing_files_and_dangling_links() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("note.txt");
    fs::write(&source, "original").unwrap();
    compression::compress(&source, ArchiveFormat::Gzip).unwrap();
    let archive = root.path().join("note.txt.gz");
    assert_eq!(
        extract(&archive).unwrap_err().kind(),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(fs::read(&source).unwrap(), b"original");
    fs::remove_file(&source).unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("missing", &source).unwrap();
        assert!(extract(&archive).is_err());
        fs::remove_file(&source).unwrap();
    }
    extract(&archive).unwrap();
    assert_eq!(fs::read(source).unwrap(), b"original");
    assert!(archive.exists());
}

#[test]
fn unsafe_zip_names_and_links_leave_no_output() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("bad.zip");
    for name in [
        "../outside",
        "/outside",
        "nested/../../outside",
        "nested\\outside",
    ] {
        zip(&path, name, false);
        assert!(extract(&path).is_err(), "{name}");
        assert!(!root.path().join("bad").exists());
    }
    zip(&path, "link", true);
    assert!(extract(&path).is_err());
    assert!(!root.path().join("bad").exists());
    assert!(!root.path().join("outside").exists());
}

#[test]
fn corrupted_archives_leave_no_destination_or_staging_files() {
    let root = tempfile::tempdir().unwrap();
    for suffix in ["zip", "gz", "tar.gz"] {
        let path = root.path().join(format!("broken.{suffix}"));
        fs::write(&path, "broken").unwrap();
        assert!(extract(&path).is_err());
        assert!(!root.path().join("broken").exists());
        fs::remove_file(&path).unwrap();
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    }
    // Truncated gzip can decode bytes before failing: those bytes stay private.
    let source = root.path().join("payload");
    fs::write(&source, "hello").unwrap();
    compression::compress(&source, ArchiveFormat::Gzip).unwrap();
    fs::remove_file(&source).unwrap();
    let archive = root.path().join("payload.gz");
    let mut bytes = fs::read(&archive).unwrap();
    bytes.truncate(bytes.len() - 4);
    fs::write(&archive, bytes).unwrap();
    assert!(extract(&archive).is_err());
    assert!(!source.exists());
}

// Build a standard ustar record without relying on tar's creation-time path
// normalization, so hostile member names and link types reach the extractor.
fn tar_gz(path: &Path, name: &str, kind: u8) {
    let mut header = [0u8; 512];
    header[..name.len()].copy_from_slice(name.as_bytes());
    header[100..108].copy_from_slice(b"0000644\0");
    header[108..116].copy_from_slice(b"0000000\0");
    header[116..124].copy_from_slice(b"0000000\0");
    header[124..136].copy_from_slice(b"00000000000\0");
    header[136..148].copy_from_slice(b"00000000000\0");
    header[148..156].fill(b' ');
    header[156] = kind;
    if matches!(kind, b'1' | b'2') {
        header[157..167].copy_from_slice(b"../outside");
    }
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    let sum: u32 = header.iter().map(|byte| u32::from(*byte)).sum();
    header[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
    let mut writer =
        flate2::write::GzEncoder::new(File::create(path).unwrap(), flate2::Compression::default());
    writer.write_all(&header).unwrap();
    writer.write_all(&[0u8; 1024]).unwrap();
    writer.finish().unwrap();
}

#[test]
fn tar_listings_accept_local_encoding_and_keep_safety_checks() {
    // Windows tar's café.txt listing in a local code page, with CRLF lines.
    let verbose = b"drwxrwxrwx 0 0 0 folder/\r\n-rw-rw-rw- 0 0 5 folder/caf\xe9.txt\r\n";
    let names = b"folder/\r\nfolder/caf\xe9.txt\r\n";
    validate_tar_listings(verbose, names).unwrap();
    validate_tar_listings(verbose, "folder/café.txt\n".as_bytes()).unwrap();
    for name in [
        &b"../caf\xe9.txt\r\n"[..],
        &b"folder/../../caf\xe9.txt\r\n"[..],
        &b"/caf\xe9.txt\r\n"[..],
        &b"C:/caf\xe9.txt\r\n"[..],
        &b"folder\\caf\xe9.txt\r\n"[..],
    ] {
        assert!(validate_tar_listings(verbose, name).is_err());
    }
    for kind in *b"lhpbc" {
        let mut listing = verbose.to_vec();
        listing[0] = kind;
        assert!(validate_tar_listings(&listing, names).is_err());
    }
}

#[test]
fn tar_rejects_traversal_links_and_special_files_before_publishing() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("bad.tar.gz");
    for (name, kind) in [
        ("../outside", b'0'),
        ("/outside", b'0'),
        ("C:/outside", b'0'),
        ("link", b'2'),
        ("hardlink", b'1'),
        ("fifo", b'6'),
    ] {
        tar_gz(&path, name, kind);
        assert!(extract(&path).is_err(), "{name}");
        assert!(!root.path().join("bad").exists());
        assert!(!root.path().join("outside").exists());
    }
}

#[test]
fn recognizes_supported_suffixes_and_rejects_empty_destination_names() {
    for name in ["sample.zip", "sample.GZ", "sample.tar.gz", "sample.TGZ"] {
        assert!(supported(Path::new(name)));
        assert!(destination(Path::new(name)).is_ok());
    }
    for name in ["file.txt", "old.tar.xz", "old.tar.bz2"] {
        assert!(!supported(Path::new(name)));
        assert!(destination(Path::new(name)).is_err());
    }
    for name in [".zip", "..gz", ".tar.gz"] {
        assert!(destination(Path::new(name)).is_err());
    }
}

#[test]
fn zip_single_file_extracts_back_to_a_file_and_multiple_roots_get_a_folder() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("note.txt");
    fs::write(&source, "hello").unwrap();
    compression::compress(&source, ArchiveFormat::Zip).unwrap();
    fs::remove_file(&source).unwrap();
    extract(&root.path().join("note.txt.zip")).unwrap();
    assert_eq!(fs::read(&source).unwrap(), b"hello");
    let archive = root.path().join("received.zip");
    let mut writer = zip::ZipWriter::new(File::create(&archive).unwrap());
    for name in ["one.txt", "two.txt"] {
        writer
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(name.as_bytes()).unwrap();
    }
    writer.finish().unwrap();
    extract(&archive).unwrap();
    for name in ["one.txt", "two.txt"] {
        assert_eq!(
            fs::read(root.path().join("received").join(name)).unwrap(),
            name.as_bytes()
        );
    }
}
