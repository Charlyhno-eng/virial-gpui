use super::*;

#[test]
fn zip_previews_count_folders_highlight_code_and_keep_images_alive() {
    use std::io::Write;
    use zip::{ZipWriter, write::SimpleFileOptions};
    let root = tempfile::tempdir().unwrap();
    let archive = root.path().join("preview.zip");
    let mut writer = ZipWriter::new(std::fs::File::create(&archive).unwrap());
    for (name, contents) in [
        ("folder/code.rs", b"fn main() {}".as_slice()),
        ("text.txt", "é".repeat(40 * 1024).as_bytes()),
        ("binary.bin", b"binary\0data".as_slice()),
        (
            "image.svg",
            b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>".as_slice(),
        ),
        ("nested.zip", b"PK\x03\x04".as_slice()),
    ] {
        writer
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(contents).unwrap();
    }
    writer.finish().unwrap();
    let entry = Entry {
        path: archive.clone(),
        name: "preview.zip".into(),
        directory: false,
        bytes: None,
    };
    assert!(entry.browsable());
    assert!(matches!(read_preview(&entry, true), Preview::Folder(5)));
    let entries = crate::infrastructure::storage::read_directory(&archive, true).unwrap();
    let find = |name: &str| entries.iter().find(|entry| entry.name == name).unwrap();
    assert!(matches!(
        read_preview(find("folder"), true),
        Preview::Folder(1)
    ));
    assert!(
        matches!(read_preview(find("text.txt"), true), Preview::Text(text) if text.len() == 64 * 1024)
    );
    assert!(matches!(
        read_preview(find("binary.bin"), true),
        Preview::Unavailable
    ));
    assert!(!find("nested.zip").browsable());
    let code = crate::infrastructure::storage::read_directory(&archive.join("folder"), true)
        .unwrap()
        .remove(0);
    assert!(
        matches!(read_preview(&code, true), Preview::Code(code) if !code.highlights.is_empty())
    );
    let Preview::ArchiveImage(image) = read_preview(find("image.svg"), true) else {
        panic!("expected ZIP image preview");
    };
    assert!(image.path.exists());
    let path = image.path.clone();
    drop(image);
    assert!(!path.exists());
}

#[test]
fn previews_bound_text_reject_binary_and_count_folder_contents() {
    let root = std::env::temp_dir().join(format!("virial-preview-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let entry = Entry {
        path: root.join("text.txt"),
        name: "text.txt".into(),
        directory: false,
        bytes: None,
    };
    std::fs::write(&entry.path, "a".repeat(70 * 1024)).unwrap();
    assert!(matches!(read_preview(&entry, false), Preview::Text(text) if text.len() == 64 * 1024));
    std::fs::write(&entry.path, "é".repeat(40 * 1024)).unwrap();
    assert!(matches!(read_preview(&entry, false), Preview::Text(text) if text.len() == 64 * 1024));
    std::fs::write(&entry.path, format!("{}é", "a".repeat(64 * 1024 - 1))).unwrap();
    assert!(
        matches!(read_preview(&entry, false), Preview::Text(text) if text.len() == 64 * 1024 - 1)
    );
    std::fs::write(&entry.path, b"binary\0data").unwrap();
    assert!(matches!(read_preview(&entry, false), Preview::Unavailable));
    let folder = Entry {
        directory: true,
        path: root.clone(),
        ..entry
    };
    assert!(matches!(read_preview(&folder, false), Preview::Folder(1)));
    std::fs::write(root.join(".hidden"), "hidden").unwrap();
    assert!(matches!(read_preview(&folder, false), Preview::Folder(1)));
    assert!(matches!(read_preview(&folder, true), Preview::Folder(2)));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn code_previews_stay_bounded_reject_binary_and_leave_the_file_unchanged() {
    let path = std::env::temp_dir().join(format!("virial-code-preview-{}.rs", std::process::id()));
    let entry = Entry {
        path: path.clone(),
        name: "main.rs".into(),
        directory: false,
        bytes: None,
    };
    let source = format!(
        "fn main() {{\n\tlet text = \"hello\";\n}}\n{}",
        "// comment\n".repeat(7000)
    );
    std::fs::write(&path, &source).unwrap();
    let Preview::Code(code) = read_preview(&entry, false) else {
        panic!("expected highlighted code");
    };
    assert_eq!(code.text.len(), 64 * 1024 + 3);
    assert!(!code.highlights.is_empty());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
    std::fs::write(&path, b"binary\0data").unwrap();
    assert!(matches!(read_preview(&entry, false), Preview::Unavailable));
    std::fs::remove_file(path).unwrap();
}
