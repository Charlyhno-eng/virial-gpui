//! Archive creation without overwriting existing files.
use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchiveFormat {
    Zip,
    TarGz,
    TarXz,
    TarBz2,
}
impl ArchiveFormat {
    pub const ALL: [Self; 4] = [Self::Zip, Self::TarGz, Self::TarXz, Self::TarBz2];
    pub fn label(self) -> &'static str {
        match self {
            Self::Zip => "ZIP (.zip)",
            Self::TarGz => "TAR.GZ (.tar.gz)",
            Self::TarXz => "TAR.XZ (.tar.xz)",
            Self::TarBz2 => "TAR.BZ2 (.tar.bz2)",
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Self::Zip => ".zip",
            Self::TarGz => ".tar.gz",
            Self::TarXz => ".tar.xz",
            Self::TarBz2 => ".tar.bz2",
        }
    }
}
pub fn destination(path: &Path, format: ArchiveFormat) -> io::Result<PathBuf> {
    let mut name = path
        .file_name()
        .ok_or_else(|| io::Error::other("No file name"))?
        .to_os_string();
    name.push(format.extension());
    Ok(path.with_file_name(name))
}
pub fn compress(path: &Path, format: ArchiveFormat) -> io::Result<()> {
    let archive = destination(path, format)?;
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&archive)?;
    let result = if format == ArchiveFormat::Zip {
        let mut writer = zip::ZipWriter::new(file);
        add_zip(
            &mut writer,
            path,
            path.parent()
                .ok_or_else(|| io::Error::other("No parent directory"))?,
        )
        .and_then(|()| writer.finish().map(|_| ()).map_err(io::Error::other))
    } else {
        let flag = match format {
            ArchiveFormat::TarGz => "-czf",
            ArchiveFormat::TarXz => "-cJf",
            ArchiveFormat::TarBz2 => "-cjf",
            ArchiveFormat::Zip => unreachable!(),
        };
        super::operations::command(
            Command::new("tar")
                .arg(flag)
                .arg("-")
                .arg("-C")
                .arg(
                    path.parent()
                        .ok_or_else(|| io::Error::other("No parent directory"))?,
                )
                .arg("--")
                .arg(path.file_name().unwrap())
                .stdout(file),
        )
    };
    if result.is_err() {
        let _ = fs::remove_file(archive);
    }
    result
}
fn add_zip(writer: &mut zip::ZipWriter<File>, path: &Path, parent: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    let relative = path.strip_prefix(parent).map_err(io::Error::other)?;
    let name = relative
        .to_str()
        .ok_or_else(|| io::Error::other("ZIP names must be UTF-8"))?
        .replace('\\', "/");
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    #[cfg(unix)]
    let options = {
        use std::os::unix::fs::PermissionsExt;
        options.unix_permissions(metadata.permissions().mode())
    };
    if metadata.is_symlink() {
        let target = fs::read_link(path)?;
        let target = target
            .to_str()
            .ok_or_else(|| io::Error::other("ZIP link targets must be UTF-8"))?;
        writer
            .add_symlink(name, target, options)
            .map_err(io::Error::other)?;
    } else if metadata.is_dir() {
        writer
            .add_directory(format!("{name}/"), options)
            .map_err(io::Error::other)?;
        for entry in fs::read_dir(path)? {
            add_zip(writer, &entry?.path(), parent)?;
        }
    } else if metadata.is_file() {
        writer.start_file(name, options).map_err(io::Error::other)?;
        io::copy(&mut File::open(path)?, writer)?;
    } else {
        return Err(io::Error::other("Unsupported file type in ZIP"));
    }
    Ok(())
}
