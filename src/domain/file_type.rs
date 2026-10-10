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
}
