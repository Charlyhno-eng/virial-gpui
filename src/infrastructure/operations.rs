//! Filesystem mutations. Never overwrite a destination or follow links while copying.
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read},
    path::{Component, Path, PathBuf},
    process::Command,
};
#[cfg(unix)]
use std::{
    ffi::CString,
    os::fd::AsRawFd,
    os::unix::{
        ffi::OsStrExt,
        fs::{OpenOptionsExt, symlink},
    },
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
    #[cfg(unix)]
    {
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
    // Windows: no atomic no-replace rename in std; keep the never-overwrite
    // guarantee with a check-then-rename race window instead of failing here.
    #[cfg(windows)]
    {
        if destination.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Destination already exists",
            ));
        }
        fs::rename(source, destination)
    }
}

// A source can be replaced between enumeration and opening. O_NONBLOCK prevents
// a replacement FIFO from hanging a worker, and O_NOFOLLOW retains link safety.
pub(super) fn open_regular_file(path: &Path) -> io::Result<File> {
    #[cfg(unix)]
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    #[cfg(windows)]
    let file = OpenOptions::new().read(true).open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Source is not a regular file",
        ));
    }
    Ok(file)
}

pub(super) fn copy(source: &Path, destination: &Path) -> io::Result<()> {
    copy_with_progress(source, destination, None)
}

pub(super) fn copy_with_progress(
    source: &Path,
    destination: &Path,
    progress: Option<&super::progress::Progress>,
) -> io::Result<()> {
    if let Some(progress) = progress {
        progress.checkpoint()?;
    }
    let metadata = fs::symlink_metadata(source)?;
    #[cfg(unix)]
    if metadata.is_symlink() {
        symlink(fs::read_link(source)?, destination)?;
        if let Some(progress) = progress {
            progress.advance(1);
        }
        return Ok(());
    }
    #[cfg(windows)]
    if metadata.is_symlink() {
        let _ = destination;
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Symlinks cannot be copied on this platform yet",
        ));
    }
    if metadata.is_dir() {
        fs::create_dir(destination)?;
        let result = (|| {
            for item in fs::read_dir(source)? {
                let item = item?;
                copy_with_progress(&item.path(), &destination.join(item.file_name()), progress)?;
            }
            fs::set_permissions(destination, metadata.permissions())
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(destination);
        }
        if result.is_ok() {
            if let Some(progress) = progress {
                progress.advance(1);
            }
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
        let mut input = open_regular_file(source)?;
        copy_contents(&mut input, &mut output, metadata.len(), progress)?;
        if let Some(progress) = progress {
            progress.advance(1);
        }
        output.set_permissions(metadata.permissions())
    })();
    if result.is_err() {
        let _ = fs::remove_file(destination);
    }
    result
}

// Keep progress updates bounded while retaining std::io::copy's Linux kernel
// offload and its fallback for unsupported filesystems/syscalls. Take<File>
// preserves that specialization; a manual read/write loop does not.
const COPY_CHUNK: u64 = 4 * 1024 * 1024;

fn copy_chunks(
    input: &mut File,
    output: &mut File,
    size: u64,
    progress: Option<&super::progress::Progress>,
) -> io::Result<()> {
    let mut remaining = size;
    while remaining > 0 {
        if let Some(progress) = progress {
            progress.checkpoint()?;
        }
        let copied = io::copy(&mut input.take(remaining.min(COPY_CHUNK)), output)?;
        if copied == 0 {
            return Err(io::Error::other("Source changed during transfer"));
        }
        remaining -= copied;
        if let Some(progress) = progress {
            progress.advance(copied);
        }
    }
    Ok(())
}

fn copy_contents(
    input: &mut File,
    output: &mut File,
    size: u64,
    progress: Option<&super::progress::Progress>,
) -> io::Result<()> {
    // Whole-file CoW cloning avoids copying blocks (including sparse holes) for
    // transfers and undo snapshots. Small files are cheaper to copy directly.
    #[cfg(unix)]
    if size >= 128 * 1024 {
        loop {
            // Both descriptors remain open. The destination was created with
            // create_new, so this ioctl cannot overwrite a user's existing file.
            let result =
                unsafe { libc::ioctl(output.as_raw_fd(), libc::FICLONE, input.as_raw_fd()) };
            if result == 0 {
                if output.metadata()?.len() != size || input.metadata()?.len() != size {
                    return Err(io::Error::other("Source changed during transfer"));
                }
                if let Some(progress) = progress {
                    progress.advance(size);
                }
                return Ok(());
            }
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            match error.raw_os_error() {
                Some(
                    libc::EOPNOTSUPP
                    | libc::ENOTTY
                    | libc::EXDEV
                    | libc::EINVAL
                    | libc::ENOSYS
                    | libc::EPERM,
                ) => break,
                // Disk-full and I/O failures remain errors, and the caller
                // removes the partial destination before a move removes source.
                _ => return Err(error),
            }
        }
    }
    copy_chunks(input, output, size, progress)?;
    if input.metadata()?.len() != size {
        return Err(io::Error::other("Source changed during transfer"));
    }
    Ok(())
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
    transfer_with_progress(sources, directory, cut, None)
}

fn transfer_with_progress(
    sources: Vec<PathBuf>,
    directory: PathBuf,
    cut: bool,
    progress: Option<&super::progress::Progress>,
) -> io::Result<()> {
    if let Some(progress) = progress {
        progress.begin(
            if cut {
                super::progress::Phase::Moving
            } else {
                super::progress::Phase::Copying
            },
            None,
        );
    }
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
    let weights = transfers
        .iter()
        .map(|(source, _)| {
            if progress.is_some() {
                super::progress::weight(source)
            } else {
                Ok(0)
            }
        })
        .collect::<io::Result<Vec<_>>>()?;
    if let Some(progress) = progress {
        progress.begin(
            if cut {
                super::progress::Phase::Moving
            } else {
                super::progress::Phase::Copying
            },
            Some(weights.iter().sum()),
        );
    }
    for ((source, destination), weight) in transfers.into_iter().zip(weights) {
        if cut {
            match rename(&source, &destination) {
                Ok(()) => {
                    if let Some(progress) = progress {
                        progress.advance(weight);
                    }
                    continue;
                }
                #[cfg(unix)]
                Err(error) if error.raw_os_error() == Some(libc::EXDEV) => {}
                #[cfg(windows)]
                Err(error) if error.raw_os_error() == Some(17) => {}
                Err(error) => return Err(error),
            }
        }
        copy_with_progress(&source, &destination, progress)?;
        if cut {
            remove(&source)?;
        }
    }
    Ok(())
}

pub fn execute(operation: Operation) -> io::Result<Option<super::archive::Materialized>> {
    execute_with_progress(operation, None)
}

pub(super) fn execute_with_progress(
    operation: Operation,
    progress: Option<&super::progress::Progress>,
) -> io::Result<Option<super::archive::Materialized>> {
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
            if let Some(progress) = progress {
                progress.begin(
                    if *cut {
                        super::progress::Phase::Moving
                    } else {
                        super::progress::Phase::Copying
                    },
                    None,
                );
            }
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
        } => {
            if progress.is_some() {
                transfer_with_progress(sources, directory, cut, progress)
            } else {
                transfer(sources, directory, cut)
            }
        }
        Operation::Trash(paths) => trash_paths(&paths),
        Operation::Launch { desktop, file } => {
            return launch(&desktop, &file);
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

const APOSTROPHE: char = '\u{27}';

// Trash via the desktop service. Windows moves items to the Recycle Bin with
// PowerShell's FileSystem API (no extra crate); other targets fail cleanly.
fn trash_paths(paths: &[PathBuf]) -> io::Result<()> {
    #[cfg(unix)]
    {
        command(Command::new("gio").arg("trash").arg("--").args(paths))
    }
    #[cfg(windows)]
    {
        // One verb per item: files and folders need different .NET calls.
        let quoted = paths
            .iter()
            .map(|path| format!("'{}'", path.to_string_lossy().replace(APOSTROPHE, "''")))
            .collect::<Vec<_>>()
            .join(", ");
        let script = format!(
            "$ErrorActionPreference='Stop'; Add-Type -AssemblyName Microsoft.VisualBasic; foreach ($item in @({quoted})) {{ if (Test-Path -LiteralPath $item -PathType Container) {{ [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteDirectory($item, 'OnlyErrorDialogs', 'SendToRecycleBin') }} else {{ [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile($item, 'OnlyErrorDialogs', 'SendToRecycleBin') }} }}",
        );
        let status = Command::new("powershell")
            .arg("-NoProfile")
            .arg("-NonInteractive")
            .arg("-Command")
            .arg(script)
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(io::Error::other("Recycle Bin move failed"))
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = paths;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Trash is not supported on this platform yet",
        ))
    }
}

// Launch a file with its default application.
fn launch(desktop: &Path, file: &Path) -> io::Result<Option<super::archive::Materialized>> {
    let extracted = if super::archive::is_member(file) {
        Some(super::archive::materialize(file, u64::MAX)?)
    } else {
        None
    };
    let file = extracted
        .as_ref()
        .map(|file| file.path.as_path())
        .unwrap_or(file);
    #[cfg(unix)]
    {
        command(Command::new("gio").arg("launch").arg(desktop).arg(file))?;
    }
    #[cfg(windows)]
    {
        // The .desktop entry is a Linux concept; the shell resolves defaults.
        let _ = desktop;
        let target = file.to_string_lossy().replace(APOSTROPHE, "''");
        command(Command::new("powershell").args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!("Start-Process -FilePath '{target}'"),
        ]))?;
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (desktop, file);
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Launch is not supported on this platform yet",
        ));
    }
    Ok(extracted)
}
#[cfg(test)]
#[path = "../../tests/infrastructure/operations_portable.rs"]
mod portable_tests;

#[cfg(all(test, unix))]
#[path = "../../tests/infrastructure/operations.rs"]
mod tests;
