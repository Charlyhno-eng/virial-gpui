use std::os::unix::ffi::OsStrExt;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

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

pub fn read_directory(path: &Path, hidden: bool) -> io::Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for item in fs::read_dir(path)? {
        let item = item?;
        if !hidden && item.file_name().as_bytes().starts_with(b".") {
            continue;
        }
        // Follow directory symlinks, but retain broken links in the listing.
        let metadata = fs::metadata(item.path()).ok();
        entries.push(Entry {
            path: item.path(),
            name: item.file_name().to_string_lossy().into_owned(),
            directory: metadata.as_ref().is_some_and(|m| m.is_dir()),
            bytes: metadata.filter(|m| m.is_file()).map(|m| m.len()),
        });
    }
    entries.sort_by(|a, b| {
        b.directory
            .cmp(&a.directory)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.path.cmp(&b.path))
    });
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::{ffi::OsStringExt, fs::symlink};

    #[test]
    fn lists_directories_first_and_handles_hidden_files_and_links() {
        let root = std::env::temp_dir().join(format!("virial-test-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("z-folder")).unwrap();
        fs::write(root.join("a-file"), b"hello").unwrap();
        fs::write(root.join(".hidden"), b"").unwrap();
        fs::write(root.join(std::ffi::OsString::from_vec(vec![0xff])), b"").unwrap();
        symlink(root.join("z-folder"), root.join("linked-folder")).unwrap();
        symlink(root.join("missing"), root.join("broken-link")).unwrap();
        let entries = read_directory(&root, false).unwrap();
        assert_eq!(entries.len(), 5);
        assert!(entries[0].directory && entries[1].directory);
        assert_eq!(entries[2].name, "a-file");
        assert_eq!(entries[2].bytes, Some(5));
        assert_eq!(read_directory(&root, true).unwrap().len(), 6);
        assert!(read_directory(&root.join("a-file"), false).is_err());
        assert!(read_directory(&root.join("missing"), false).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
