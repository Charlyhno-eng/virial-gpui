use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub directory: bool,
    pub bytes: Option<u64>,
}

impl Entry {
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
            "rs" | "py" | "js" | "ts" | "html" | "css" | "json" | "toml" | "sh" | "c" | "cpp" => {
                "Source code"
            }
            "txt" | "md" | "pdf" | "odt" | "doc" | "docx" => "Document",
            _ => "File",
        }
    }

    pub fn icon(&self) -> &'static str {
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
