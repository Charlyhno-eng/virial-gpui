//! Archive extraction stages all contents before publishing a new destination.
use std::{
    fs::{self, File},
    io,
    path::{Component, Path, PathBuf},
    process::Command,
};

#[derive(Clone, Copy)]
enum Format {
    Zip,
    Gzip,
    TarGzip,
}

fn format(path: &Path) -> Option<Format> {
    let name = path.file_name()?.to_str()?.to_ascii_lowercase();
    if name.ends_with(".zip") {
        Some(Format::Zip)
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        Some(Format::TarGzip)
    } else if name.ends_with(".gz") {
        Some(Format::Gzip)
    } else {
        None
    }
}

pub fn supported(path: &Path) -> bool {
    format(path).is_some()
}

pub fn destination(path: &Path) -> io::Result<PathBuf> {
    let format = format(path).ok_or_else(|| io::Error::other("Unsupported archive format"))?;
    let name = path.file_name().and_then(|name| name.to_str()).unwrap();
    let suffix = match format {
        Format::Zip => 4,
        Format::Gzip => 3,
        Format::TarGzip if name.to_ascii_lowercase().ends_with(".tar.gz") => 7,
        Format::TarGzip => 4,
    };
    super::operations::named_path(
        path.parent()
            .ok_or_else(|| io::Error::other("No parent directory"))?,
        &name[..name.len() - suffix],
    )
}

pub fn extract(path: &Path) -> io::Result<()> {
    let destination = destination(path)?;
    // Resolve the parent so relative names cannot be interpreted as tar options.
    let parent = destination.parent().unwrap().canonicalize()?;
    let destination = parent.join(destination.file_name().unwrap());
    match fs::symlink_metadata(&destination) {
        Ok(_) => return Err(io::ErrorKind::AlreadyExists.into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let staging = tempfile::Builder::new()
        .prefix(".virial-extract-")
        .tempdir_in(parent)?;
    let output = staging.path().join("contents");
    match format(path).unwrap() {
        Format::Zip => {
            fs::create_dir(&output)?;
            super::archive::extract_all(path, &output)?;
        }
        Format::Gzip => {
            let mut decoder = flate2::read::MultiGzDecoder::new(File::open(path)?);
            io::copy(&mut decoder, &mut File::create(&output)?)?;
        }
        Format::TarGzip => {
            // Work from one immutable snapshot for validation and extraction.
            let tar = staging.path().join("archive.tar");
            let mut decoder = flate2::read::MultiGzDecoder::new(File::open(path)?);
            io::copy(&mut decoder, &mut File::create(&tar)?)?;
            validate_tar(&tar)?;
            fs::create_dir(&output)?;
            super::operations::command(
                tar_command()
                    .arg("-xf")
                    .arg(&tar)
                    .arg("-C")
                    .arg(&output)
                    .arg("--no-same-owner"),
            )?;
        }
    }
    // Archives created from one file or folder already carry its name. Avoid
    // wrapping that entry in another folder with the same name. Multi-entry
    // archives still get a containing directory named after the archive.
    let mut published = output.clone();
    if output.is_dir() {
        let entries = fs::read_dir(&output)?.collect::<io::Result<Vec<_>>>()?;
        if entries.len() == 1 && entries[0].file_name() == destination.file_name().unwrap() {
            published = entries[0].path();
        }
    }
    super::operations::rename(&published, &destination)
}

fn tar_command() -> Command {
    let mut command = Command::new("tar");
    command.env_remove("TAR_OPTIONS").env(
        "LC_ALL",
        if cfg!(target_os = "macos") {
            "en_US.UTF-8"
        } else {
            "C.UTF-8"
        },
    );
    command
}

fn listing(path: &Path, flag: &str) -> io::Result<String> {
    let output = tar_command().arg(flag).arg(path).output()?;
    if !output.status.success() {
        return Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).trim(),
        ));
    }
    String::from_utf8(output.stdout).map_err(io::Error::other)
}

fn validate_tar(path: &Path) -> io::Result<()> {
    // GNU tar and bsdtar both list the entry kind first. Refuse links and
    // special files before extraction, including links used as parent folders.
    for line in listing(path, "-tvf")?.lines() {
        if !matches!(line.as_bytes().first(), Some(b'-' | b'd')) {
            return Err(io::Error::other(
                "Archive links and special files cannot be extracted",
            ));
        }
    }
    for name in listing(path, "-tf")?.lines() {
        let path = Path::new(name);
        if name.contains(['\\', ':'])
            || path
                .components()
                .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
        {
            return Err(io::Error::other("Unsafe archive member path"));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/infrastructure/extraction.rs"]
mod tests;
