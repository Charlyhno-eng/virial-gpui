//! Filesystem mutations. Never overwrite a destination or follow links while copying.
use std::{
    ffi::CString,
    fs::{self, OpenOptions},
    io,
    os::unix::{ffi::OsStrExt, fs::symlink},
    path::{Component, Path, PathBuf},
    process::Command,
};

#[derive(Clone, Debug)]
pub enum Operation {
    Transfer {
        sources: Vec<PathBuf>,
        directory: PathBuf,
        cut: bool,
    },
    Rename {
        source: PathBuf,
        name: String,
    },
    New {
        directory: PathBuf,
        name: String,
        folder: bool,
    },
    Trash(Vec<PathBuf>),
    Compress(PathBuf),
    ImageExport {
        source: PathBuf,
        name: String,
        edit: super::image_edit::ImageEdit,
    },
    Launch {
        desktop: PathBuf,
        file: PathBuf,
    },
}

pub fn named_path(directory: &Path, name: &str) -> io::Result<PathBuf> {
    let mut components = Path::new(name).components();
    if name.is_empty()
        || name.contains('/')
        || name.contains('\0')
        || !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid file name",
        ));
    }
    Ok(directory.join(name))
}

pub(super) fn rename(source: &Path, destination: &Path) -> io::Result<()> {
    let source = CString::new(source.as_os_str().as_bytes())?;
    let destination = CString::new(destination.as_os_str().as_bytes())?;
    // Linux atomic no-replace rename also protects against a concurrent creator.
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

pub(super) fn copy(source: &Path, destination: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(source)?;
    if metadata.is_symlink() {
        return symlink(fs::read_link(source)?, destination);
    }
    if metadata.is_dir() {
        fs::create_dir(destination)?;
        let result = (|| {
            for item in fs::read_dir(source)? {
                let item = item?;
                copy(&item.path(), &destination.join(item.file_name()))?;
            }
            fs::set_permissions(destination, metadata.permissions())
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(destination);
        }
        return result;
    }
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Special files cannot be copied",
        ));
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    let result = (|| {
        io::copy(&mut fs::File::open(source)?, &mut output)?;
        output.set_permissions(metadata.permissions())
    })();
    if result.is_err() {
        let _ = fs::remove_file(destination);
    }
    result
}

pub(super) fn remove(source: &Path) -> io::Result<()> {
    if fs::symlink_metadata(source)?.is_dir() {
        fs::remove_dir_all(source)
    } else {
        fs::remove_file(source)
    }
}

fn command(command: &mut Command) -> io::Result<()> {
    let output = command.output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "{}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

pub(super) fn transfer(sources: Vec<PathBuf>, directory: PathBuf, cut: bool) -> io::Result<()> {
    let directory = directory.canonicalize()?;
    if !directory.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            "Destination is not a folder",
        ));
    }
    // Resolve parent aliases without following the selected item itself (it may be a link).
    let mut sources = sources
        .into_iter()
        .map(|source| {
            let name = source
                .file_name()
                .ok_or_else(|| io::Error::other("No file name"))?;
            let parent = source
                .parent()
                .filter(|path| !path.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            Ok(parent.canonicalize()?.join(name))
        })
        .collect::<io::Result<Vec<_>>>()?;
    sources.sort();
    sources.dedup();
    let folders = sources
        .iter()
        .filter_map(|source| {
            fs::symlink_metadata(source)
                .ok()
                .filter(|metadata| metadata.is_dir())
                .map(|_| source.clone())
        })
        .collect::<std::collections::HashSet<_>>();
    // A selected parent carries its descendants along; do not transfer them twice.
    sources.retain(|source| {
        !source
            .ancestors()
            .skip(1)
            .any(|parent| folders.contains(parent))
    });
    let mut targets = std::collections::HashSet::new();
    let mut transfers = Vec::new();
    for source in sources {
        let metadata = fs::symlink_metadata(&source)?;
        if metadata.is_dir() && directory.starts_with(source.canonicalize()?) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Cannot transfer a folder into itself",
            ));
        }
        let destination = directory.join(source.file_name().unwrap());
        if cut && source == destination {
            continue;
        }
        if !targets.insert(destination.clone()) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Selected items have the same destination name",
            ));
        }
        match fs::symlink_metadata(&destination) {
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("{} already exists", destination.display()),
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        transfers.push((source, destination));
    }
    // Validate every destination before starting; atomic no-replace operations
    // still protect against files created after this preflight.
    for (source, destination) in transfers {
        if cut {
            match rename(&source, &destination) {
                Ok(()) => continue,
                Err(error) if error.raw_os_error() == Some(libc::EXDEV) => {}
                Err(error) => return Err(error),
            }
        }
        copy(&source, &destination)?;
        if cut {
            remove(&source)?;
        }
    }
    Ok(())
}

pub fn execute(operation: Operation) -> io::Result<Option<super::archive::Materialized>> {
    match &operation {
        Operation::Rename { source, name } if super::archive::is_member(source) => {
            return super::archive::rename(source, name).map(|_| None);
        }
        Operation::Transfer {
            sources,
            directory,
            cut,
        } if super::archive::split(directory).is_some()
            || sources.iter().any(|path| super::archive::is_member(path)) =>
        {
            return super::archive::transfer(sources.clone(), directory.clone(), *cut)
                .map(|_| None);
        }
        Operation::New {
            directory,
            name,
            folder,
        } if super::archive::split(directory).is_some() => {
            return super::archive::create(directory, name, *folder).map(|_| None);
        }
        Operation::Trash(paths) if paths.iter().any(|path| super::archive::is_member(path)) => {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "ZIP members cannot be moved to the desktop Trash",
            ));
        }
        _ => {}
    }
    match operation {
        Operation::ImageExport { source, name, edit } => {
            super::image_edit::export(&source, &name, edit)
        }
        Operation::Rename { source, name } => rename(
            &source,
            &named_path(
                source
                    .parent()
                    .ok_or_else(|| io::Error::other("No parent directory"))?,
                &name,
            )?,
        ),
        Operation::New {
            directory,
            name,
            folder,
        } => {
            let destination = named_path(&directory, &name)?;
            if folder {
                fs::create_dir(destination)
            } else {
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(destination)
                    .map(|_| ())
            }
        }
        Operation::Transfer {
            sources,
            directory,
            cut,
        } => transfer(sources, directory, cut),
        Operation::Trash(paths) => command(Command::new("gio").arg("trash").arg("--").args(paths)),
        Operation::Launch { desktop, file } => {
            let extracted = if super::archive::is_member(&file) {
                Some(super::archive::materialize(&file, u64::MAX)?)
            } else {
                None
            };
            let file = extracted
                .as_ref()
                .map(|file| file.path.as_path())
                .unwrap_or(&file);
            command(Command::new("gio").arg("launch").arg(desktop).arg(file))?;
            return Ok(extracted);
        }
        Operation::Compress(path) => {
            let name = path
                .file_name()
                .ok_or_else(|| io::Error::other("No file name"))?;
            let mut archive = name.to_os_string();
            archive.push(".tar.gz");
            let archive = path.with_file_name(archive);
            let file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&archive)?;
            let result = command(
                Command::new("tar")
                    .arg("-czf")
                    .arg("-")
                    .arg("-C")
                    .arg(
                        path.parent()
                            .ok_or_else(|| io::Error::other("No parent directory"))?,
                    )
                    .arg("--")
                    .arg(name)
                    .stdout(file),
            );
            if result.is_err() {
                let _ = fs::remove_file(archive);
            }
            result
        }
    }
    .map(|_| None)
}

#[cfg(test)]
#[path = "../../tests/infrastructure/operations.rs"]
mod tests;
