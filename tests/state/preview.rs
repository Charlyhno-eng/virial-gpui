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
