use super::*;

fn entry(name: &str, directory: bool) -> Entry {
    Entry {
        path: name.into(),
        name: name.into(),
        directory,
        bytes: None,
    }
}

#[test]
fn extension_icons_are_case_insensitive_and_keep_fallbacks() {
    for (name, expected) in [
        ("main.py", "python"),
        ("main.PYW", "python"),
        ("app.ts", "typescript"),
        ("App.TSX", "react"),
        ("app.jsx", "react"),
        ("index.mjs", "javascript"),
        ("main.rs", "rust"),
        ("index.html", "html"),
        ("style.scss", "css"),
        ("data.json", "json"),
        ("settings.toml", "config"),
        ("settings.yml", "config"),
        ("run.sh", "shell"),
        ("main.c", "c"),
        ("main.hpp", "cpp"),
        ("report.PDF", "pdf"),
        ("README.md", "markdown"),
        ("photo.png", "image"),
        ("song.mp3", "music"),
        ("backup.tar.gz", "archive"),
        ("notes.txt", "file"),
        ("unknown.xyz", "file"),
        ("LICENSE", "file"),
        (".hidden", "file"),
    ] {
        assert_eq!(entry(name, false).icon(), expected, "{name}");
        assert_eq!(entry(name, true).icon(), "folder", "{name}");
    }
    assert_eq!(entry("App.tsx", false).kind(), "Source code");
}

#[test]
fn specialized_icons_are_embedded_valid_svg_assets() {
    use gpui::AssetSource;
    let assets = crate::ui::icons::IconAssets;
    for extension in [
        "py", "js", "ts", "tsx", "rs", "html", "css", "json", "toml", "sh", "c", "cpp", "pdf", "md",
    ] {
        let name = entry(&format!("file.{extension}"), false).icon();
        let bytes = assets.load(&format!("icons/{name}.svg")).unwrap().unwrap();
        let svg = std::str::from_utf8(&bytes).unwrap();
        let document = roxmltree::Document::parse(svg).unwrap();
        assert_eq!(document.root_element().tag_name().name(), "svg");
        assert!(document.descendants().any(|node| node.has_tag_name("path")));
    }
}
