//! File-type artwork for the browser, the home screen and the picker.
//!
//! Resolution order: exact filename (Dockerfile, Makefile, ...), then a long
//! extension table, then the kind reported by the scanner, then a generic
//! document. Keeping the order explicit here means callers never re-implement
//! the mapping: `file_icon(entry.icon(), size)` is the only entry point.

/// Artwork for a filename: exact matches win over the extension table, which in
/// turn wins over nothing. Names are lower-cased before matching.
pub fn icon_for(name: &str) -> &'static str {
    match name.to_ascii_lowercase().as_str() {
        "dockerfile" | "containerfile" => return "docker",
        "makefile" | "gnumakefile" | "justfile" => return "config",
        "cmakelists.txt" => return "config",
        ".gitignore" | ".gitattributes" | ".gitmodules" => return "git",
        ".dockerignore" => return "docker",
        ".env" | ".env.local" | ".env.production" | ".env.development" => return "config",
        ".editorconfig" => return "config",
        ".npmrc" | ".yarnrc" => return "config",
        "cargo.toml" => return "rust",
        "cargo.lock" => return "rust",
        "package.json" | "package-lock.json" => return "javascript",
        "yarn.lock" | "pnpm-lock.yaml" | "bun.lockb" => return "config",
        "tsconfig.json" => return "typescript",
        "readme" | "readme.md" | "readme.txt" => return "markdown",
        "changelog" | "changelog.md" => return "markdown",
        "authors" | "contributors" => return "markdown",
        _ => {}
    }
    icon_for_extension(name.rsplit('.').next().unwrap_or(name))
}

/// Artwork for a bare extension (no dot). Unknown extensions fall back to the
/// generic document so the list never renders an empty cell.
pub fn icon_for_extension(extension: &str) -> &'static str {
    match extension.to_ascii_lowercase().as_str() {
        // Languages
        "rs" => "rust",
        "py" | "pyw" | "pyi" | "rpy" => "python",
        "js" | "mjs" | "cjs" => "javascript",
        "jsx" | "tsx" => "react",
        "ts" | "mts" | "cts" => "typescript",
        "go" => "code",
        "java" | "jar" => "code",
        "kt" | "kts" => "code",
        "swift" => "code",
        "rb" | "gemspec" => "code",
        "php" => "code",
        "lua" => "code",
        "hs" | "lhs" => "code",
        "ex" | "exs" => "code",
        "erl" | "hrl" => "code",
        "zig" => "code",
        "dart" => "code",
        "scala" | "sc" => "code",
        "pl" | "pm" => "code",
        "r" | "rmd" => "code",
        "jl" => "code",
        "c" | "h" | "m" | "mm" => "c",
        "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" | "ipp" => "cpp",
        "cs" => "code",
        "fs" | "fsx" => "code",
        "clj" | "cljs" | "edn" => "code",
        "sql" => "code",
        "sh" | "bash" | "zsh" | "fish" | "ksh" => "shell",
        "ps1" | "psm1" | "psd1" | "bat" | "cmd" => "shell",
        // Web
        "html" | "htm" | "xhtml" | "vue" | "svelte" => "html",
        "css" | "pcss" => "css",
        "scss" | "sass" | "less" => "sass",
        "json" | "jsonc" | "json5" => "json",
        "xml" | "xsd" | "xsl" | "xslt" | "plist" | "svg" => "code",
        "yaml" | "yml" => "config",
        "toml" | "ini" | "conf" | "cfg" | "properties" | "env" => "config",
        // Documents and text
        "md" | "mdx" | "markdown" | "rst" | "adoc" | "log" => "markdown",
        "pdf" => "pdf",
        "doc" | "docx" | "odt" | "rtf" => "markdown",
        // Data
        "csv" | "tsv" | "parquet" | "db" | "sqlite" | "sqlite3" => "code",
        // Archives
        "zip" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "7z" | "rar" | "zst" | "lz4" | "iso"
        | "cab" => "archive",
        // Media
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "avif" | "heic" | "heif" | "tiff"
        | "tif" | "ico" | "icns" | "xcf" | "psd" | "raw" | "cr2" | "nef" | "arw" | "dng"
        | "orf" | "rw2" => "image",
        "mp3" | "flac" | "wav" | "ogg" | "oga" | "opus" | "m4a" | "aac" | "wma" | "aiff"
        | "ape" | "mid" | "midi" => "music",
        "mp4" | "mkv" | "webm" | "avi" | "mov" | "wmv" | "flv" | "m4v" | "mpg" | "mpeg" | "3gp"
        | "vob" => "video",
        // Fonts
        "ttf" | "otf" | "woff" | "woff2" | "eot" => "code",
        // Executables and installers
        "exe" | "msi" | "appx" | "msix" => "shell",
        "dmg" | "app" | "pkg" | "ipa" => "archive",
        "deb" | "rpm" | "apk" | "appimage" | "flatpak" | "snap" | "nix" => "archive",
        "wasm" => "code",
        _ => "file",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_filenames_beat_extensions() {
        assert_eq!(icon_for("Dockerfile"), "docker");
        assert_eq!(icon_for("dockerfile"), "docker");
        assert_eq!(icon_for("Containerfile"), "docker");
        assert_eq!(icon_for("Makefile"), "config");
        assert_eq!(icon_for("Justfile"), "config");
        assert_eq!(icon_for("CMakeLists.txt"), "config");
        assert_eq!(icon_for(".gitignore"), "git");
        assert_eq!(icon_for(".env"), "config");
        assert_eq!(icon_for("Cargo.toml"), "rust");
        assert_eq!(icon_for("cargo.lock"), "rust");
        assert_eq!(icon_for("package-lock.json"), "javascript");
        assert_eq!(icon_for("LICENSE"), "file");
    }

    #[test]
    fn extension_table_covers_the_common_families() {
        let cases = [
            ("main.rs", "rust"),
            ("script.py", "python"),
            ("index.tsx", "react"),
            ("app.ts", "typescript"),
            ("style.scss", "sass"),
            ("style.css", "css"),
            ("page.html", "html"),
            ("data.json", "json"),
            ("settings.yml", "config"),
            ("notes.md", "markdown"),
            ("report.pdf", "pdf"),
            ("song.flac", "music"),
            ("clip.mkv", "video"),
            ("photo.heic", "image"),
            ("backup.tar.gz", "archive"),
            ("tool.exe", "shell"),
            ("installer.dmg", "archive"),
            ("main.c", "c"),
            ("main.cpp", "cpp"),
        ];
        for (name, expected) in cases {
            assert_eq!(icon_for(name), expected, "{name}");
        }
    }

    #[test]
    fn unknown_files_fall_back_to_a_generic_document() {
        assert_eq!(icon_for("mystery.qqqq"), "file");
        assert_eq!(icon_for("noextension"), "file");
        assert_eq!(icon_for_extension(""), "file");
    }

    #[test]
    fn every_icon_used_is_available_in_the_catalog() {
        // Guards against a typo in the table pointing at an asset that does not
        // exist: the browser would silently render nothing for those rows.
        for name in [
            "docker",
            "git",
            "rust",
            "python",
            "javascript",
            "typescript",
            "react",
            "code",
            "c",
            "cpp",
            "shell",
            "html",
            "css",
            "sass",
            "json",
            "config",
            "markdown",
            "pdf",
            "image",
            "music",
            "video",
            "archive",
            "file",
            "folder",
        ] {
            assert!(
                crate::ui::icons::has_asset(name),
                "missing asset for {name}"
            );
        }
    }

    // --- exhaustive coverage of the two tables -----------------------------------
    //
    // The hand-written list above can only prove the icons it names. These tests
    // drive every single key in the table so that adding a row with a typo fails the
    // suite instead of rendering an empty cell.

    /// Every extension the table knows about. Kept in one place so a new row is
    /// obvious when it is missing from the coverage tests.
    const EVERY_EXTENSION: &[&str] = &[
        "3gp",
        "7z",
        "aac",
        "adoc",
        "aiff",
        "ape",
        "apk",
        "app",
        "appimage",
        "appx",
        "arw",
        "avi",
        "avif",
        "bash",
        "bat",
        "bmp",
        "bz2",
        "c",
        "cab",
        "cc",
        "cfg",
        "cjs",
        "clj",
        "cljs",
        "cmd",
        "conf",
        "cpp",
        "cr2",
        "cs",
        "css",
        "csv",
        "cts",
        "cxx",
        "dart",
        "db",
        "deb",
        "dmg",
        "dng",
        "doc",
        "docx",
        "edn",
        "env",
        "eot",
        "erl",
        "ex",
        "exe",
        "exs",
        "fish",
        "flac",
        "flatpak",
        "flv",
        "fs",
        "fsx",
        "gemspec",
        "gif",
        "go",
        "gz",
        "h",
        "heic",
        "heif",
        "hh",
        "hpp",
        "hrl",
        "hs",
        "htm",
        "html",
        "hxx",
        "icns",
        "ico",
        "ini",
        "ipa",
        "ipp",
        "iso",
        "jar",
        "java",
        "jl",
        "jpeg",
        "jpg",
        "js",
        "json",
        "json5",
        "jsonc",
        "jsx",
        "ksh",
        "kt",
        "kts",
        "less",
        "lhs",
        "log",
        "lua",
        "lz4",
        "m",
        "m4a",
        "m4v",
        "markdown",
        "md",
        "mdx",
        "mid",
        "midi",
        "mjs",
        "mkv",
        "mm",
        "mov",
        "mp3",
        "mp4",
        "mpeg",
        "mpg",
        "msi",
        "msix",
        "mts",
        "nef",
        "nix",
        "odt",
        "oga",
        "ogg",
        "opus",
        "orf",
        "otf",
        "parquet",
        "pcss",
        "pdf",
        "php",
        "pkg",
        "pl",
        "plist",
        "pm",
        "png",
        "properties",
        "ps1",
        "psd",
        "psd1",
        "psm1",
        "py",
        "pyi",
        "pyw",
        "r",
        "rar",
        "raw",
        "rb",
        "rmd",
        "rpm",
        "rpy",
        "rs",
        "rst",
        "rtf",
        "rw2",
        "sass",
        "sc",
        "scala",
        "scss",
        "sh",
        "snap",
        "sql",
        "sqlite",
        "sqlite3",
        "svelte",
        "svg",
        "swift",
        "tar",
        "tgz",
        "tif",
        "tiff",
        "toml",
        "ts",
        "tsv",
        "tsx",
        "ttf",
        "vob",
        "vue",
        "wasm",
        "wav",
        "webm",
        "webp",
        "wma",
        "wmv",
        "woff",
        "woff2",
        "xcf",
        "xhtml",
        "xml",
        "xsd",
        "xsl",
        "xslt",
        "xz",
        "yaml",
        "yml",
        "zig",
        "zip",
        "zsh",
        "zst",
    ];

    /// Every exact filename the table knows about, including the dotfile spellings.
    const EVERY_EXACT_NAME: &[&str] = &[
        ".dockerignore",
        ".editorconfig",
        ".env",
        ".env.development",
        ".env.local",
        ".env.production",
        ".gitattributes",
        ".gitignore",
        ".gitmodules",
        ".npmrc",
        ".yarnrc",
        "authors",
        "bun.lockb",
        "cargo.lock",
        "cargo.toml",
        "changelog",
        "changelog.md",
        "cmakelists.txt",
        "containerfile",
        "contributors",
        "dockerfile",
        "gnumakefile",
        "justfile",
        "makefile",
        "package-lock.json",
        "package.json",
        "pnpm-lock.yaml",
        "readme",
        "readme.md",
        "readme.txt",
        "tsconfig.json",
        "yarn.lock",
    ];

    /// Every icon name the two functions can hand back.
    const EVERY_ICON_NAME: &[&str] = &[
        "archive",
        "c",
        "code",
        "config",
        "cpp",
        "css",
        "docker",
        "file",
        "git",
        "html",
        "image",
        "javascript",
        "json",
        "markdown",
        "music",
        "pdf",
        "python",
        "react",
        "rust",
        "sass",
        "shell",
        "typescript",
        "video",
    ];

    #[test]
    fn every_extension_maps_to_a_shipped_asset() {
        for extension in EVERY_EXTENSION {
            let icon = icon_for_extension(extension);
            assert!(
                crate::ui::icons::has_asset(icon),
                "extension {extension:?} maps to {icon:?}, which has no asset"
            );
            // A row that resolves to the fallback would be silently dead weight.
            assert_ne!(
                icon, "file",
                "extension {extension:?} falls through to the fallback"
            );
        }
    }

    #[test]
    fn every_exact_name_maps_to_a_shipped_asset() {
        for name in EVERY_EXACT_NAME {
            let icon = icon_for(name);
            assert!(
                crate::ui::icons::has_asset(icon),
                "name {name:?} maps to {icon:?}, which has no asset"
            );
            assert_ne!(icon, "file", "name {name:?} falls through to the fallback");
        }
    }

    #[test]
    fn the_documented_icon_universe_is_exactly_what_the_tables_reach() {
        // Pins the reachable set of icon names, so a new row that introduces a brand
        // new artwork file fails here instead of needing a silent asset addition.
        let mut reached: Vec<&str> = EVERY_EXTENSION
            .iter()
            .map(|extension| icon_for_extension(extension))
            .chain(EVERY_EXACT_NAME.iter().map(|name| icon_for(name)))
            .chain(["file"])
            .collect();
        reached.sort_unstable();
        reached.dedup();
        assert_eq!(reached, EVERY_ICON_NAME);
    }

    #[test]
    fn every_extension_is_reachable_through_icon_for() {
        // `icon_for` falls back to `rsplit('.')`, so a bare extension must resolve
        // the same way whether it arrives on its own or behind a dot.
        for extension in EVERY_EXTENSION {
            assert_eq!(
                icon_for(&format!("file.{extension}")),
                icon_for_extension(extension),
                "file.{extension} disagrees with the bare extension"
            );
        }
    }

    #[test]
    fn extensions_are_matched_case_insensitively() {
        for extension in ["RS", "Py", "MDX", "JPG", "Pdf", "YAML", "WASM", "Zig"] {
            assert_eq!(
                icon_for_extension(extension),
                icon_for_extension(&extension.to_ascii_lowercase()),
                "{extension} is not lower-cased before matching"
            );
        }
    }

    #[test]
    fn exact_names_are_matched_case_insensitively() {
        for name in [
            "Dockerfile",
            "DOCKERFILE",
            "CONTAINERFILE",
            "Makefile",
            "GNUMAKEFILE",
            "Justfile",
            "CMakeLists.txt",
            ".gitignore",
            ".GITMODULES",
            ".Env.Production",
            ".EditorConfig",
            "Cargo.toml",
            "Cargo.Lock",
            "Package.json",
            "TSConfig.json",
            "Readme.MD",
            "CHANGELOG.md",
            "pnpm-lock.yaml",
        ] {
            assert_eq!(
                icon_for(name),
                icon_for(&name.to_ascii_lowercase()),
                "{name} is not lower-cased before matching"
            );
        }
    }

    #[test]
    fn the_exact_name_table_wins_over_the_extension_table() {
        // "Makefile" must not fall through to the extension table: `rsplit('.')`
        // would hand "makefile" to `icon_for_extension`, which returns "file".
        assert_eq!(icon_for("Makefile"), "config");
        assert_eq!(icon_for("Makefile.am"), "file");
        // Likewise a dotfile that also parses as an extension.
        assert_eq!(icon_for(".env"), "config");
        assert_eq!(icon_for_extension("env"), "config");
        // "Cargo.toml" would otherwise be a plain "config" row.
        assert_eq!(icon_for("Cargo.toml"), "rust");
        assert_eq!(icon_for_extension("toml"), "config");
        // "package.json" beats the generic "json" row.
        assert_eq!(icon_for("package.json"), "javascript");
        assert_eq!(icon_for_extension("json"), "json");
    }

    #[test]
    fn only_the_last_extension_segment_decides() {
        // The dotfile cases are the reason this is worth pinning: `rsplit('.')` on
        // ".env.local" yields "local", so the exact-name table is the only reason
        // the file gets real artwork.
        assert_eq!(icon_for(".env.local"), "config");
        assert_eq!(icon_for(".env.local"), icon_for(".env"));
        assert_eq!(icon_for("archive.tar.gz"), "archive");
        assert_eq!(icon_for("notes.md.bak"), "file");
        assert_eq!(icon_for("a.b.rs"), "rust");
    }

    #[test]
    fn names_without_an_extension_fall_back_to_a_generic_document() {
        for name in [
            "LICENSE",
            "Makefile.in",
            "Dockerfile.dev",
            "justfile.am",
            "notes",
        ] {
            assert_eq!(icon_for(name), "file", "{name}");
        }
        assert_eq!(icon_for(""), "file");
        assert_eq!(icon_for("."), "file");
        assert_eq!(icon_for(".."), "file");
        assert_eq!(icon_for_extension(""), "file");
        assert_eq!(icon_for_extension("."), "file");
    }
}
