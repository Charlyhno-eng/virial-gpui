use super::*;

#[test]
fn files_take_two_units_and_closing_them_reveals_the_previous_parent() {
    for depth in 0..30 {
        let start = visible_column_start(depth, true);
        let folders = depth + 1 - start;
        assert_eq!(folders, (depth + 1).min(2));
        assert!(folders + 2 <= MAX_PREVIEW_COLUMNS);
        if depth > 0 {
            // Closing the file returns to the cached folder and restores parents.
            assert_eq!(
                visible_column_start(depth - 1, false),
                start.saturating_sub(2)
            );
        }
    }
}

#[test]
fn keyboard_selection_previews_nested_markdown_and_retains_each_branch_selection() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("folder/deep/empty")).unwrap();
    std::fs::write(
        root.path().join("folder/deep/readme.md"),
        "# Résumé\n\n**Document**",
    )
    .unwrap();
    let entry = Entry {
        path: root.path().to_path_buf(),
        name: "root".into(),
        directory: true,
        bytes: None,
    };
    let mut columns = Vec::new();
    let mut current = entry.clone();
    for _ in 0..3 {
        let Preview::Folder(entries) = read_preview(&current, true) else {
            panic!("folder preview")
        };
        let mut column = PreviewColumn {
            entry: current,
            entries,
            scroll: gpui::UniformListScrollHandle::new(),
            selection: Default::default(),
        };
        current = keyboard_preview_entry(
            &mut column.selection,
            &column.entries,
            &column.scroll,
            "down",
        )
        .unwrap();
        columns.push(column);
    }
    assert_eq!(current.name, "empty");
    assert!(matches!(read_preview(&current, true), Preview::Folder(entries) if entries.is_empty()));
    let column = columns.last_mut().unwrap();
    let markdown = keyboard_preview_entry(
        &mut column.selection,
        &column.entries,
        &column.scroll,
        "down",
    )
    .unwrap();
    assert_eq!(markdown.name, "readme.md");
    assert!(
        matches!(read_preview(&markdown, true), Preview::Markdown(document) if !document.rendered.is_empty())
    );
    assert_eq!(columns[0].selection.primary(), Some(0));
    assert_eq!(columns[1].selection.primary(), Some(0));
    assert_eq!(columns[2].selection.primary(), Some(1));
    assert_eq!(columns[0].entry.path, entry.path);
    PreviewColumn::branch(&mut columns, 1);
    assert_eq!(columns.len(), 2);
    assert_eq!(columns[1].selection.primary(), Some(0));
    let column = columns.last_mut().unwrap();
    let deep = keyboard_preview_entry(&mut column.selection, &column.entries, &column.scroll, "up")
        .unwrap();
    assert_eq!(deep.name, "deep");
    assert!(matches!(read_preview(&deep, true), Preview::Folder(entries) if entries.len() == 2));
    assert!(
        keyboard_preview_entry(
            &mut Default::default(),
            &[],
            &gpui::UniformListScrollHandle::new(),
            "down"
        )
        .is_none()
    );
}

#[test]
fn real_project_readme_is_a_formatted_document() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("README.md");
    let entry = Entry {
        path,
        name: "README.md".into(),
        directory: false,
        bytes: None,
    };
    let Preview::Markdown(document) = read_preview(&entry, true) else {
        panic!("formatted README")
    };
    assert!(
        document
            .rendered
            .iter()
            .any(|line| matches!(line.kind, super::super::markdown::LineKind::Heading(_)))
    );
    assert!(!document.rendered.is_empty());
}

#[test]
fn deep_navigation_hides_parents_and_reveals_them_when_closing_levels() {
    for depth in 0..30 {
        let start = visible_column_start(depth, false);
        let shown = (start..depth + 2).collect::<Vec<_>>();
        assert_eq!(shown.len(), (depth + 2).min(4));
        assert_eq!(shown.last(), Some(&(depth + 1)));
    }
    let root = tempfile::tempdir().unwrap();
    let mut path = root.path().to_path_buf();
    let mut columns = Vec::new();
    for _ in 0..6 {
        path.push("child");
        std::fs::create_dir(&path).unwrap();
        columns.push(PreviewColumn {
            entry: Entry {
                path: path.clone(),
                name: "child".into(),
                directory: true,
                bytes: None,
            },
            entries: Vec::new(),
            scroll: gpui::UniformListScrollHandle::new(),
            selection: Default::default(),
        });
    }
    let original = columns[0].entry.path.clone();
    assert!(visible_column_start(columns.len(), false) > 1);
    while columns.len() > 1 {
        columns.pop();
    }
    assert_eq!(visible_column_start(columns.len(), false), 0);
    assert_eq!(columns[0].entry.path, original);
}

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
    assert!(matches!(read_preview(&entry, true), Preview::Folder(entries) if entries.len() == 5));
    let entries = crate::infrastructure::storage::read_directory(&archive, true).unwrap();
    let find = |name: &str| entries.iter().find(|entry| entry.name == name).unwrap();
    assert!(matches!(
        read_preview(find("folder"), true),
        Preview::Folder(entries) if entries.len() == 1
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
    let Preview::ArchiveImage(image, _) = read_preview(find("image.svg"), true) else {
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
    assert!(matches!(read_preview(&folder, false), Preview::Folder(entries) if entries.len() == 1));
    std::fs::write(root.join(".hidden"), "hidden").unwrap();
    assert!(matches!(read_preview(&folder, false), Preview::Folder(entries) if entries.len() == 1));
    assert!(matches!(read_preview(&folder, true), Preview::Folder(entries) if entries.len() == 2));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn markdown_and_html_get_a_rendered_view_and_everything_else_does_not() {
    let root = tempfile::tempdir().unwrap();
    let markdown = "# Title\n\nBody";
    let html = "<html><head><title>Home</title></head><body><h1>T</h1></body></html>";
    let cases = [
        ("notes.md", markdown, true),
        ("notes.markdown", markdown, true),
        ("NOTES.MD", markdown, true),
        ("page.html", html, true),
        ("page.htm", html, true),
        ("page.xhtml", html, true),
        ("PAGE.HTML", html, true),
    ];
    for (name, contents, rendered) in cases {
        let path = root.path().join(name);
        std::fs::write(&path, contents).unwrap();
        let entry = Entry {
            path,
            name: name.into(),
            directory: false,
            bytes: None,
        };
        // Both documents open on the rendered view; the extension match is
        // case-insensitive, unlike the plain `Code` route.
        assert_eq!(
            matches!(
                read_preview(&entry, false),
                Preview::Markdown(_) | Preview::Html(_, true)
            ),
            rendered,
            "{name} opens on its rendered view"
        );
    }
    // A file whose grammar is missing still gets its source view.
    let path = root.path().join("page.weird");
    std::fs::write(&path, html).unwrap();
    let entry = Entry {
        path,
        name: "page.weird".into(),
        directory: false,
        bytes: None,
    };
    assert!(matches!(read_preview(&entry, false), Preview::Code(_)));
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

#[test]
fn pdf_previews_render_local_and_zip_pages_and_clean_up() {
    use std::io::Write;
    // This integration check needs the documented optional PDF renderer.
    if std::process::Command::new("pdftoppm")
        .arg("-v")
        .output()
        .is_err()
    {
        eprintln!("Skipping PDF rendering check: pdftoppm is unavailable");
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("sample.PDF");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /Resources << >> /Contents 4 0 R >>",
        "<< /Length 0 >>\nstream\n\nendstream",
    ];
    let mut pdf = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.push_str(&format!("{} 0 obj\n{object}\nendobj\n", index + 1));
    }
    let xref = pdf.len();
    pdf.push_str("xref\n0 5\n0000000000 65535 f \n");
    for offset in offsets {
        pdf.push_str(&format!("{offset:010} 00000 n \n"));
    }
    pdf.push_str(&format!(
        "trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n"
    ));
    std::fs::write(&path, &pdf).unwrap();
    let archive = root.path().join("sample.zip");
    let mut writer = zip::ZipWriter::new(File::create(&archive).unwrap());
    writer
        .start_file("sample.PDF", zip::write::SimpleFileOptions::default())
        .unwrap();
    writer.write_all(pdf.as_bytes()).unwrap();
    writer.finish().unwrap();
    for path in [path.clone(), archive.join("sample.PDF")] {
        let entry = Entry {
            path: path.clone(),
            name: "sample.PDF".into(),
            directory: false,
            bytes: None,
        };
        let Preview::Pdf(rendered) = read_preview(&entry, false) else {
            panic!("expected PDF preview for {path:?}");
        };
        let image = image::open(&rendered.path).unwrap();
        assert_eq!((image.width(), image.height()), (1067, 1600));
        let rendered_path = rendered.path.clone();
        drop(rendered);
        assert!(!rendered_path.exists());
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), pdf);
    std::fs::write(&path, b"invalid PDF").unwrap();
    assert!(pdf_preview(&path).is_err());
    File::create(&path)
        .unwrap()
        .set_len(20 * 1024 * 1024 + 1)
        .unwrap();
    assert!(pdf_preview(&path).is_err());
}

#[test]
fn image_previews_read_header_metadata_locally_and_in_zip() {
    use std::io::Write;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("image.png");
    image::RgbaImage::new(37, 23).save(&path).unwrap();
    let archive = root.path().join("images.zip");
    let mut writer = zip::ZipWriter::new(File::create(&archive).unwrap());
    writer
        .start_file("image.png", zip::write::SimpleFileOptions::default())
        .unwrap();
    writer.write_all(&std::fs::read(&path).unwrap()).unwrap();
    writer.finish().unwrap();
    for path in [path.clone(), archive.join("image.png")] {
        let entry = Entry {
            path,
            name: "image.png".into(),
            directory: false,
            bytes: None,
        };
        let preview = read_preview(&entry, false);
        let metadata = preview.image_metadata().expect("image metadata");
        assert_eq!(metadata.format, "Png");
        assert_eq!((metadata.width, metadata.height), (37, 23));
    }
    std::fs::write(&path, b"invalid image").unwrap();
    let entry = Entry {
        path,
        name: "image.png".into(),
        directory: false,
        bytes: None,
    };
    let preview = read_preview(&entry, false);
    assert!(matches!(preview, Preview::Image(..)));
    assert!(preview.image_metadata().is_none());
}

#[test]
fn media_previews_route_local_and_zip_audio_and_video_and_reject_special_files() {
    use std::io::Write;
    let root = tempfile::tempdir().unwrap();
    let archive = root.path().join("media.zip");
    let mut writer = zip::ZipWriter::new(File::create(&archive).unwrap());
    for name in ["track.MP3", "clip.MP4"] {
        writer
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"invalid media").unwrap();
        std::fs::write(root.path().join(name), b"invalid media").unwrap();
    }
    writer.finish().unwrap();
    for directory in [root.path(), archive.as_path()] {
        for (name, video) in [("track.MP3", false), ("clip.MP4", true)] {
            let entry = Entry {
                path: directory.join(name),
                name: name.into(),
                directory: false,
                bytes: None,
            };
            let Preview::Media(media) = read_preview(&entry, false) else {
                panic!("expected media preview")
            };
            assert_eq!(media.video, video);
            assert!(media.snapshot().paused);
        }
    }
    let missing = Entry {
        path: root.path().join("missing.mp3"),
        name: "missing.mp3".into(),
        directory: false,
        bytes: None,
    };
    assert!(matches!(
        read_preview(&missing, false),
        Preview::Unavailable
    ));
    let fifo = root.path().join("special.wav");
    let name = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    let special = Entry {
        path: fifo,
        name: "special.wav".into(),
        directory: false,
        bytes: None,
    };
    assert!(matches!(
        read_preview(&special, false),
        Preview::Unavailable
    ));
}

#[test]
fn folder_previews_can_follow_children_and_preview_their_files() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("child/grandchild/empty")).unwrap();
    std::fs::write(root.path().join("child/grandchild/note.txt"), "document").unwrap();
    let selected = Entry {
        path: root.path().to_path_buf(),
        name: "root".into(),
        directory: true,
        bytes: None,
    };
    let Preview::Folder(children) = read_preview(&selected, true) else {
        panic!("folder listing")
    };
    assert_eq!(children[0].name, "child");
    let Preview::Folder(grandchildren) = read_preview(&children[0], true) else {
        panic!("child listing")
    };
    assert_eq!(grandchildren[0].name, "grandchild");
    let Preview::Folder(contents) = read_preview(&grandchildren[0], true) else {
        panic!("grandchild listing")
    };
    assert_eq!(contents[0].name, "empty");
    assert!(
        matches!(read_preview(&contents[0], true), Preview::Folder(entries) if entries.is_empty())
    );
    assert!(matches!(read_preview(&contents[1], true), Preview::Text(text) if text == "document"));
    // Reading a nested preview leaves the original selection available.
    assert_eq!(selected.path, root.path());
    assert!(
        matches!(read_preview(&selected, true), Preview::Folder(entries) if entries[0].name == "child")
    );
}

#[test]
fn nested_columns_keep_parent_contents_and_scroll_when_switching_branches() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("first/deeper/last")).unwrap();
    std::fs::create_dir_all(root.path().join("second/other")).unwrap();
    let folder = |path: std::path::PathBuf| Entry {
        name: path.file_name().unwrap().to_string_lossy().into_owned(),
        path,
        directory: true,
        bytes: None,
    };
    let selected = folder(root.path().to_path_buf());
    let mut columns = Vec::new();
    for entry in [
        selected.clone(),
        folder(root.path().join("first")),
        folder(root.path().join("first/deeper")),
    ] {
        let Preview::Folder(entries) = read_preview(&entry, true) else {
            panic!("folder listing")
        };
        columns.push(PreviewColumn {
            entry,
            entries,
            scroll: gpui::UniformListScrollHandle::new(),
            selection: Default::default(),
        });
    }
    let scroll = columns[0].scroll.clone();
    scroll
        .0
        .borrow()
        .base_handle
        .set_offset(gpui::point(gpui::px(0.), gpui::px(-36.)));
    assert_eq!(columns[0].entry.path, selected.path);
    assert_eq!(columns[1].entries[0].name, "deeper");
    assert_eq!(columns[2].entries[0].name, "last");
    // Clicking another child in the first column removes only deeper columns.
    let sibling = columns[0]
        .entries
        .iter()
        .find(|entry| entry.name == "second")
        .unwrap()
        .clone();
    PreviewColumn::branch(&mut columns, 0);
    assert_eq!(columns.len(), 1);
    assert_eq!(columns[0].entry.path, selected.path);
    assert_eq!(columns[0].entries.len(), 2);
    assert_eq!(
        columns[0].scroll.0.borrow().base_handle.offset().y,
        gpui::px(-36.)
    );
    assert!(
        matches!(read_preview(&sibling, true), Preview::Folder(entries) if entries[0].name == "other")
    );
    // Returning uses the retained parent without disturbing the main selection.
    assert_eq!(columns.pop().unwrap().entry.path, selected.path);
}
