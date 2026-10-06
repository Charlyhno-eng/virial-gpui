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
    assert_eq!(color_at(&code, "fn"), rgb(0xff5cce).into());
    assert_eq!(color_at(&code, "héllo"), rgb(0x5cf3ff).into());
    assert_eq!(color_at(&code, "42"), rgb(0xc792ff).into());
    assert_eq!(color_at(&code, "comment"), rgb(0x8f86b8).into());
    assert_eq!(color_at(&code, "continued"), rgb(0x8f86b8).into());
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
    assert_eq!(color_at(&code, "1;"), rgb(0xc792ff).into());
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
    assert_eq!(color_at(&code, "hello"), rgb(0x5cf3ff).into());
    let code =
        CodePreview::new(Path::new("main.ts"), "const message: string = \"hello\";\n").unwrap();
    assert_eq!(color_at(&code, "hello"), rgb(0x5cf3ff).into());
    assert_ne!(color_at(&code, "const"), rgb(CODE_TEXT).into());
}

#[test]
fn recognizes_onig_only_and_aliased_grammars() {
    // PowerShell and JSX grammars ship only in the onig syntax set.
    for name in ["run.ps1", "module.psm1", "manifest.psd1", "component.jsx"] {
        assert!(CodePreview::new(Path::new(name), "").is_some(), "{name}");
    }
    let code = CodePreview::new(
        Path::new("deploy.ps1"),
        "$name = 'server'\nWrite-Output $name\n",
    )
    .unwrap();
    assert_ne!(color_at(&code, "server"), rgb(CODE_TEXT).into());
    // Podman containers use Dockerfile's grammar.
    let code = CodePreview::new(Path::new("Containerfile"), "FROM alpine\n").unwrap();
    assert_eq!(code.text.as_ref(), "FROM alpine\n");
}

#[test]
fn highlights_the_common_code_families_of_the_syntax_set() {
    // One representative per language family present in the bundled grammars:
    // a colored token proves the syntax was matched, not just the file name.
    let sources = [
        ("main.py", "def greet():\n    return 42\n", "return"),
        ("app.go", "func main() {\n\tn := 1\n}\n", "func"),
        ("app.java", "class Main {\n    void run() {}\n}\n", "class"),
        ("lib.php", "<?php\nfunction f() { return 1; }\n", "function"),
        ("app.rb", "def f\n  42\nend\n", "def"),
        ("schema.sql", "SELECT id FROM users;\n", "SELECT"),
        ("chart.yaml", "kind: Deployment\n", "kind"),
        ("Cargo.lock", "[package]\nname = \"x\"\n", "x"),
        ("build.cmake", "project(demo)\n", "project"),
        ("main.scala", "object App { def f = 1 }\n", "object"),
        ("main.dart", "void main() { var x = 1; }\n", "void"),
        ("main.ex", "defmodule M do\nend\n", "defmodule"),
        ("main.lua", "local function f() end\n", "local"),
        ("main.hs", "main :: IO ()\nmain = return ()\n", "::"),
        ("main.clj", "(defn f [] 1)\n", "defn"),
        ("main.zig", "pub fn main() void {}\n", "pub"),
        ("main.nim", "proc f() = discard\n", "proc"),
        ("main.sol", "contract C {}\n", "contract"),
        ("main.tf", "resource \"a\" \"b\" {}\n", "resource"),
        ("main.glsl", "void main() { gl_FragColor = vec4(1.); }\n", "void"),
        ("main.scss", "$c: red;\n.a { color: $c; }\n", "red"),
        ("conf.toml", "key = 1\n", "key"),
        ("host.nginx.conf", "server { listen 80; }\n", "server"),
    ];
    for (name, source, token) in sources {
        let code = CodePreview::new(Path::new(name), source)
            .unwrap_or_else(|| panic!("{name} has no grammar"));
        assert_ne!(
            color_at(&code, token),
            rgb(CODE_TEXT).into(),
            "{name}: token {token} is not highlighted"
        );
    }
}

#[test]
fn archives_and_config_files_with_no_extension_still_get_a_grammar() {
    let sources = [
        ("Dockerfile", "FROM debian\n"),
        ("Containerfile", "FROM debian\n"),
        ("Makefile", "all:\n\techo ok\n"),
        ("CMakeLists.txt", "project(demo)\n"),
        ("Rakefile", "task :default\n"),
        (".gitignore", "target/\n"),
        (".gitattributes", "*.png text\n"),
        ("COMMIT_EDITMSG", "fix: sample\n"),
        ("crontab", "0 * * * * echo\n"),
        ("fstab", "/dev/sda1 / ext4 defaults 0 1\n"),
        ("nginx.conf", "events {}\n"),
        ("CMakeCache.txt", "cache\n"),
    ];
    for (name, source) in sources {
        assert!(
            CodePreview::new(Path::new(name), source).is_some(),
            "{name} has no grammar"
        );
    }
}

#[test]
fn unknown_and_plain_text_files_keep_the_plain_preview() {
    assert!(CodePreview::new(Path::new("notes.txt"), "hello\nworld").is_none());
    assert!(CodePreview::new(Path::new("notes.unknown"), "hello\nworld").is_none());
}
