use super::*;

fn color_at(code: &CodePreview, needle: &str) -> gpui::Hsla {
    let offset = code.text.find(needle).unwrap();
    code.highlights
        .iter()
        .find(|(range, _)| range.contains(&offset))
        .and_then(|(_, style)| style.color)
        .unwrap()
}

#[test]
fn colors_keywords_strings_numbers_and_multiline_comments() {
    let source = "fn main() {\n    let message = \"héllo\";\n    let count = 42;\n    /* comment\n       continued */\n}\n";
    let code = CodePreview::new(Path::new("main.rs"), source).unwrap();
    assert_eq!(code.text.as_ref(), source);
    assert_eq!(color_at(&code, "fn"), rgb(0x569cd6).into());
    assert_eq!(color_at(&code, "héllo"), rgb(0xce9178).into());
    assert_eq!(color_at(&code, "42"), rgb(0xb5cea8).into());
    assert_eq!(color_at(&code, "comment"), rgb(0x6a9955).into());
    assert_eq!(color_at(&code, "continued"), rgb(0x6a9955).into());
    for (range, _) in &code.highlights {
        assert!(code.text.is_char_boundary(range.start));
        assert!(code.text.is_char_boundary(range.end));
    }
    for spans in code.highlights.windows(2) {
        assert_eq!(spans[0].0.end, spans[1].0.start);
    }
    assert_eq!(code.highlights.last().unwrap().0.end, code.text.len());
}

#[test]
fn preserves_indentation_blank_lines_and_long_lines() {
    let source = format!(
        "fn main() {{\r\n\tlet café = 1;\r\n\r\n \t// {}\r\n}}\r\n",
        "long line ".repeat(100)
    );
    let code = CodePreview::new(Path::new("main.RS"), &source).unwrap();
    assert_eq!(
        code.text.as_ref(),
        format!(
            "fn main() {{\n    let café = 1;\n\n    // {}\n}}\n",
            "long line ".repeat(100)
        )
    );
    assert_eq!(code.line_numbers.as_ref(), "1\n2\n3\n4\n5\n6");
    assert_eq!(color_at(&code, "1;"), rgb(0xb5cea8).into());
}

#[test]
fn recognizes_common_languages_filenames_and_shebangs() {
    for name in [
        "main.rs",
        "main.py",
        "main.js",
        "main.ts",
        "main.tsx",
        "main.c",
        "main.cpp",
        "index.html",
        "style.css",
        "data.json",
        "Cargo.toml",
        "run.sh",
        "Dockerfile",
        "Makefile",
    ] {
        assert!(CodePreview::new(Path::new(name), "").is_some(), "{name}");
    }
    let code = CodePreview::new(
        Path::new("script"),
        "#!/usr/bin/env python3\nprint(\"hello\")\n",
    )
    .unwrap();
    assert_eq!(color_at(&code, "hello"), rgb(0xce9178).into());
    let code =
        CodePreview::new(Path::new("main.ts"), "const message: string = \"hello\";\n").unwrap();
    assert_eq!(color_at(&code, "hello"), rgb(0xce9178).into());
    assert_ne!(color_at(&code, "const"), rgb(CODE_TEXT).into());
}

#[test]
fn unknown_and_plain_text_files_keep_the_plain_preview() {
    assert!(CodePreview::new(Path::new("notes.txt"), "hello\nworld").is_none());
    assert!(CodePreview::new(Path::new("notes.unknown"), "hello\nworld").is_none());
}
