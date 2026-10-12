//! Archive creation without overwriting existing files.
use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufWriter, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchiveFormat {
    Zip,
    Gzip,
}
impl ArchiveFormat {
    pub const ALL: [Self; 2] = [Self::Zip, Self::Gzip];
    pub fn label(self) -> &'static str {
        match self {
            Self::Zip => "ZIP (.zip)",
            Self::Gzip => "GZIP (.gz)",
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Self::Zip => ".zip",
            Self::Gzip => ".gz",
        }
    }
}

/// Archive names, including formats that Virial does not extract yet.
pub fn is_archive(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "zip"
                    | "gz"
                    | "gzip"
                    | "tgz"
                    | "tar"
                    | "bz2"
                    | "bzip2"
                    | "tbz"
                    | "tbz2"
                    | "xz"
                    | "txz"
                    | "zst"
                    | "tzst"
                    | "7z"
                    | "rar"
                    | "lz"
                    | "lzma"
                    | "lzo"
                    | "z"
                    | "cab"
            )
        })
}

pub fn destination(path: &Path, format: ArchiveFormat) -> io::Result<PathBuf> {
    let directory = fs::symlink_metadata(path)?.is_dir();
    if !directory && is_archive(path) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "This file is already an archive",
        ));
    }
    let mut name = path
        .file_name()
        .ok_or_else(|| io::Error::other("No file name"))?
        .to_os_string();
    name.push(if format == ArchiveFormat::Gzip && directory {
        ".tar.gz"
    } else {
        format.extension()
    });
    Ok(path.with_file_name(name))
}
#[cfg(test)]
pub fn compress(path: &Path, format: ArchiveFormat) -> io::Result<()> {
    compress_with_progress(path, format, None)
}

pub fn compress_with_progress(
    path: &Path,
    format: ArchiveFormat,
    progress: Option<&super::progress::Progress>,
) -> io::Result<()> {
    if let Some(progress) = progress {
        progress.begin(super::progress::Phase::Compressing, None);
        progress.checkpoint()?;
    }
    let archive = destination(path, format)?;
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&archive)?;
    // One reusable input buffer and one bounded output buffer, on one worker.
    let mut buffer = vec![0; 128 * 1024];
    let result = (|| {
        let output = BufWriter::with_capacity(256 * 1024, file);
        match format {
            ArchiveFormat::Zip => {
                let mut writer = zip::ZipWriter::new(output);
                add_zip(
                    &mut writer,
                    path,
                    path.parent().unwrap(),
                    progress,
                    &mut buffer,
                )?;
                if let Some(progress) = progress {
                    progress.checkpoint()?;
                }
                writer.finish()?.flush()
            }
            ArchiveFormat::Gzip => {
                let mut writer = flate2::write::GzEncoder::new(output, flate2::Compression::fast());
                if fs::symlink_metadata(path)?.is_dir() {
                    add_tar(&mut writer, path, progress, &mut buffer)?;
                } else {
                    if !fs::symlink_metadata(path)?.is_file() {
                        return Err(io::Error::other(
                            "GZIP requires a regular file or directory",
                        ));
                    }
                    copy_stream(&mut File::open(path)?, &mut writer, progress, &mut buffer)?;
                }
                if let Some(progress) = progress {
                    progress.checkpoint()?;
                }
                writer.finish()?.flush()
            }
        }
    })();
    if result.is_err() {
        let _ = fs::remove_file(archive);
    }
    result
}

fn copy_stream(
    input: &mut impl Read,
    output: &mut impl Write,
    progress: Option<&super::progress::Progress>,
    buffer: &mut [u8],
) -> io::Result<()> {
    loop {
        if let Some(progress) = progress {
            progress.checkpoint()?;
        }
        let count = input.read(buffer)?;
        if count == 0 {
            break;
        }
        output.write_all(&buffer[..count])?;
        if let Some(progress) = progress {
            progress.advance(count as u64);
            progress.transferred(count as u64, 0);
        }
    }
    Ok(())
}

fn add_tar(
    writer: &mut impl Write,
    path: &Path,
    progress: Option<&super::progress::Progress>,
    buffer: &mut [u8],
) -> io::Result<()> {
    // Let tar stream the tree; compress in this worker so cancellation also
    // stops the compressor, without leaving a separate gzip process running.
    let errors = tempfile::tempfile()?;
    let mut child = Command::new("tar")
        .env_remove("TAR_OPTIONS")
        .arg("-cf")
        .arg("-")
        .arg("-C")
        .arg(
            path.parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
        )
        .arg("--")
        .arg(path.file_name().unwrap())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(errors.try_clone()?)
        .spawn()?;
    let result = copy_stream(&mut child.stdout.take().unwrap(), writer, progress, buffer);
    if result.is_err() {
        let _ = child.kill();
    }
    let status = child.wait()?;
    result?;
    if !status.success() {
        use std::io::{Seek, SeekFrom};
        let mut errors = errors;
        errors.seek(SeekFrom::Start(0))?;
        let mut message = String::new();
        errors.take(8192).read_to_string(&mut message)?;
        return Err(io::Error::other(format!("{status}: {}", message.trim())));
    }
    Ok(())
}

fn add_zip(
    writer: &mut zip::ZipWriter<BufWriter<File>>,
    path: &Path,
    parent: &Path,
    progress: Option<&super::progress::Progress>,
    buffer: &mut [u8],
) -> io::Result<()> {
    if let Some(progress) = progress {
        progress.checkpoint()?;
    }
    let metadata = fs::symlink_metadata(path)?;
    let relative = path.strip_prefix(parent).map_err(io::Error::other)?;
    let name = relative
        .to_str()
        .ok_or_else(|| io::Error::other("ZIP names must be UTF-8"))?
        .replace('\\', "/");
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(1));
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
            add_zip(writer, &entry?.path(), parent, progress, buffer)?;
        }
    } else if metadata.is_file() {
        writer.start_file(name, options).map_err(io::Error::other)?;
        copy_stream(&mut File::open(path)?, writer, progress, buffer)?;
    } else {
        return Err(io::Error::other("Unsupported file type in ZIP"));
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/infrastructure/compression.rs"]
mod tests;
