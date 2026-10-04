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
    Paste {
        source: PathBuf,
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
    Trash(PathBuf),
    Compress(PathBuf),
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

fn rename(source: &Path, destination: &Path) -> io::Result<()> {
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

fn copy(source: &Path, destination: &Path) -> io::Result<()> {
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

fn remove(source: &Path) -> io::Result<()> {
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

pub fn execute(operation: Operation) -> io::Result<()> {
    match operation {
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
        Operation::Paste {
            source,
            directory,
            cut,
        } => {
            let directory = directory.canonicalize()?;
            let destination = directory.join(
                source
                    .file_name()
                    .ok_or_else(|| io::Error::other("No file name"))?,
            );
            if fs::symlink_metadata(&source)?.is_dir()
                && directory.starts_with(source.canonicalize()?)
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Cannot copy a folder into itself",
                ));
            }
            if cut {
                match rename(&source, &destination) {
                    Ok(()) => return Ok(()),
                    Err(error) if error.raw_os_error() == Some(libc::EXDEV) => {}
                    Err(error) => return Err(error),
                }
            }
            copy(&source, &destination)?;
            if cut {
                remove(&source)?;
            }
            Ok(())
        }
        Operation::Trash(path) => command(Command::new("gio").arg("trash").arg("--").arg(path)),
        Operation::Launch { desktop, file } => {
            command(Command::new("gio").arg("launch").arg(desktop).arg(file))
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
}

#[cfg(test)]
#[path = "../../tests/infrastructure/operations.rs"]
mod tests;
