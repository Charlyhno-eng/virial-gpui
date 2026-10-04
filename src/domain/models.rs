use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub directory: bool,
    pub bytes: Option<u64>,
}

impl Entry {
    pub fn browsable(&self) -> bool {
        self.directory
            || (crate::infrastructure::archive::is_zip(&self.path)
                && !crate::infrastructure::archive::is_member(&self.path))
    }

    pub fn kind(&self) -> &'static str {
        if self.directory {
            return "Folder";
        }
        match self
            .path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" => "Image",
            "mp3" | "flac" | "ogg" | "wav" | "m4a" => "Audio",
            "zip" | "gz" | "xz" | "tar" | "7z" | "bz2" | "zst" => "Archive",
            "rs" | "py" | "pyw" | "js" | "mjs" | "cjs" | "jsx" | "ts" | "tsx" | "html" | "htm"
            | "css" | "scss" | "sass" | "json" | "toml" | "yaml" | "yml" | "sh" | "bash" | "c"
            | "h" | "cpp" | "cc" | "cxx" | "hpp" => "Source code",
            "txt" | "md" | "pdf" | "odt" | "doc" | "docx" => "Document",
            _ => "File",
        }
    }

    pub fn icon(&self) -> &'static str {
        if self.directory {
            return "folder";
        }
        match self
            .path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "py" | "pyw" => return "python",
            "js" | "mjs" | "cjs" => return "javascript",
            "ts" => return "typescript",
            "jsx" | "tsx" => return "react",
            "rs" => return "rust",
            "html" | "htm" => return "html",
            "css" | "scss" | "sass" => return "css",
            "json" => return "json",
            "toml" | "yaml" | "yml" => return "config",
            "sh" | "bash" => return "shell",
            "c" | "h" => return "c",
            "cpp" | "cc" | "cxx" | "hpp" => return "cpp",
            "pdf" => return "pdf",
            "md" => return "markdown",
            _ => {}
        }
        match self.kind() {
            "Folder" => "folder",
            "Image" => "image",
            "Audio" => "music",
            "Archive" => "archive",
            "Source code" => "code",
            _ => "file",
        }
    }
}

pub fn format_size(bytes: Option<u64>) -> String {
    let Some(bytes) = bytes else {
        return "—".into();
    };
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = "B";
    for next in ["KiB", "MiB", "GiB", "TiB", "PiB", "EiB"] {
        value /= 1024.;
        unit = next;
        if value < 1024. {
            break;
        }
    }
    format!("{value:.1} {unit}")
}

#[cfg(test)]
#[path = "../../tests/domain/models.rs"]
mod tests;
