use super::*;

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
