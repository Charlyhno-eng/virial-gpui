use super::*;

fn format(line_ending: LineEnding, bom: bool) -> DocumentFormat {
    DocumentFormat {
        line_ending,
        bom,
        latin1_fallback: false,
    }
}

#[test]
fn detects_the_dominant_line_ending() {
    assert_eq!(LineEnding::detect("a\nb\n"), LineEnding::Lf);
    assert_eq!(LineEnding::detect("a\r\nb\r\n"), LineEnding::LfCr);
    assert_eq!(LineEnding::detect("a\rb\r"), LineEnding::Cr);
    // A mixed file follows the strict majority.
    assert_eq!(LineEnding::detect("a\r\nb\r\nc\n"), LineEnding::LfCr);
    assert_eq!(LineEnding::detect("a\nb\nc\r\n"), LineEnding::Lf);
    assert_eq!(LineEnding::detect("single line"), LineEnding::Lf);
}

#[test]
fn decodes_plain_utf8_without_a_bom() {
    let (text, format) = decode("héllo\n".as_bytes()).unwrap();
    assert_eq!(text, "héllo\n");
    assert!(!format.bom);
    assert!(!format.latin1_fallback);
}

#[test]
fn strips_and_remembers_a_utf8_bom() {
    let mut bytes = vec![0xef, 0xbb, 0xbf];
    bytes.extend_from_slice("a\n".as_bytes());
    let (text, format) = decode(&bytes).unwrap();
    assert_eq!(text, "a\n", "the BOM is not part of the text");
    assert!(format.bom);
    assert_eq!(encode(&text, &format), bytes, "and it comes back on save");
}

#[test]
fn falls_back_to_latin1_instead_of_losing_a_byte() {
    let (text, format) = decode(b"caf\xe9\n").unwrap();
    assert_eq!(text, "café\n");
    assert!(format.latin1_fallback);
    // Latin-1 round-trips: every original byte is still there.
    assert_eq!(encode(&text, &format), b"caf\xe9\n");
}

#[test]
fn refuses_binary_content() {
    assert!(decode(&[0x00, 0x01, 0x02]).is_none());
    assert!(decode(b"text\0more").is_none());
}

#[test]
fn encoding_restores_the_original_line_ending() {
    let cases = [
        (LineEnding::Lf, "a\nb\n", b"a\nb\n".to_vec()),
        (LineEnding::LfCr, "a\nb\n", b"a\r\nb\r\n".to_vec()),
        (LineEnding::Cr, "a\nb\n", b"a\rb\r".to_vec()),
    ];
    for (line_ending, text, expected) in cases {
        let bytes = encode(text, &format(line_ending, false));
        assert_eq!(bytes, expected, "{line_ending:?}");
    }
}

#[test]
fn encoding_does_not_double_carriage_returns() {
    // The buffer stores LF only; CRLF input must not become CRCRLF.
    let bytes = encode("a\nb\n", &format(LineEnding::LfCr, false));
    assert_eq!(bytes, b"a\r\nb\r\n");
    let bytes = encode("a\r\nb\r\n", &format(LineEnding::Lf, false));
    assert_eq!(bytes, b"a\nb\n");
}

#[test]
fn a_file_round_trips_byte_for_byte() {
    for original in [
        &b"plain\n"[..],
        &b"\xef\xbb\xbfwith bom\n"[..],
        &b"windows\r\nlines\r\n"[..],
        &b"caf\xe9 latin1\n"[..],
    ] {
        let (text, format) = decode(original).unwrap();
        assert_eq!(encode(&text, &format), original, "{original:?}");
    }
}

#[test]
fn fingerprint_changes_when_content_length_or_bytes_change() {
    let base = fingerprint(b"hello", None);
    assert_eq!(base.len, 5);
    assert_eq!(fingerprint(b"hello", None), base, "same input, same value");
    assert_ne!(fingerprint(b"hellp", None), base, "one byte differs");
    assert_ne!(fingerprint(b"hello!", None), base, "length differs");
}

#[test]
fn fingerprint_notices_a_same_length_rewrite() {
    let time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    let before = fingerprint(b"version one", Some(time));
    let after = fingerprint(b"version two", Some(time));
    assert_eq!(before.len, after.len, "same length");
    assert_eq!(before.modified, after.modified, "same timestamp");
    assert_ne!(
        before.hash, after.hash,
        "a same-size rewrite within the same second is still a conflict"
    );
}

#[test]
fn fingerprint_records_the_modification_second() {
    let time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_123);
    assert_eq!(fingerprint(b"x", Some(time)).modified, Some(1_700_000_123));
    assert_eq!(fingerprint(b"x", None).modified, None);
}

#[test]
fn write_atomic_creates_replaces_and_keeps_permissions() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("file.txt");

    std::fs::write(&path, b"first").unwrap();
    write_atomic(&path, b"second").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"second");

    // No temporary file is left behind next to the target.
    let leftovers: Vec<_> = std::fs::read_dir(directory.path())
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".virial-editor"))
        .collect();
    assert!(leftovers.is_empty(), "left {leftovers:?}");

    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(0o640);
        std::fs::set_permissions(&path, permissions).unwrap();
        write_atomic(&path, b"third").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o640, "mode survives the rewrite");
    }
    #[cfg(not(unix))]
    let _ = permissions;
}

#[test]
fn write_atomic_reports_a_failure_without_touching_the_original() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("file.txt");
    std::fs::write(&path, b"keep me").unwrap();
    // A directory cannot be replaced by a file rename on any platform.
    let blocked = directory.path().join("subdir");
    std::fs::create_dir(&blocked).unwrap();
    assert!(write_atomic(&blocked, b"payload").is_err());
    assert_eq!(
        std::fs::read(&path).unwrap(),
        b"keep me",
        "the sibling file is untouched"
    );
}

#[cfg(unix)]
#[test]
fn write_atomic_works_for_a_path_without_a_directory_component() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("bare.txt");
    std::fs::write(&path, b"old").unwrap();
    write_atomic(&path, b"new").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"new");
}
