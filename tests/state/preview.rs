use super::*;

#[test]
fn previews_bound_text_and_reject_binary_and_folders() {
    let root = std::env::temp_dir().join(format!("virial-preview-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let entry = Entry {
        path: root.join("text.txt"),
        name: "text.txt".into(),
        directory: false,
        bytes: None,
    };
    std::fs::write(&entry.path, "a".repeat(70 * 1024)).unwrap();
    assert!(matches!(read_preview(&entry), Preview::Text(text) if text.len() == 64 * 1024));
    std::fs::write(&entry.path, "é".repeat(40 * 1024)).unwrap();
    assert!(matches!(read_preview(&entry), Preview::Text(text) if text.len() == 64 * 1024));
    std::fs::write(&entry.path, format!("{}é", "a".repeat(64 * 1024 - 1))).unwrap();
    assert!(matches!(read_preview(&entry), Preview::Text(text) if text.len() == 64 * 1024 - 1));
    std::fs::write(&entry.path, b"binary\0data").unwrap();
    assert!(matches!(read_preview(&entry), Preview::Unavailable));
    let folder = Entry {
        directory: true,
        path: root.clone(),
        ..entry
    };
    assert!(matches!(read_preview(&folder), Preview::Unavailable));
    std::fs::remove_dir_all(root).unwrap();
}
