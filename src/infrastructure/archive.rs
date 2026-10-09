//! ZIP members use virtual paths such as `/downloads/book.zip/chapter/page.txt`.
//! Writes rebuild beside the original and replace it only after the ZIP is complete.
use crate::domain::models::Entry;
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    sync::Mutex,
};
use tempfile::{NamedTempFile, TempDir};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

static WRITE_LOCK: Mutex<()> = Mutex::new(());

pub fn is_zip(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
}

/// The archive itself remains a real file for rename/copy/trash operations.
pub fn split(path: &Path) -> Option<(PathBuf, PathBuf)> {
    path.ancestors().find_map(|parent| {
        (is_zip(parent) && parent.is_file()).then(|| {
            (
                parent.to_path_buf(),
                path.strip_prefix(parent).unwrap().to_path_buf(),
            )
        })
    })
}

pub fn is_member(path: &Path) -> bool {
    split(path).is_some_and(|(_, member)| !member.as_os_str().is_empty())
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

// Path::join("") adds a trailing slash, which turns a ZIP file record into a folder.
fn join(base: &Path, rest: &Path) -> PathBuf {
    if rest.as_os_str().is_empty() {
        base.to_path_buf()
    } else {
        base.join(rest)
    }
}

fn member_name(path: &Path) -> io::Result<String> {
    let name = path
        .to_str()
        .ok_or_else(|| invalid("ZIP names must be UTF-8"))?;
    if name.contains(['\\', '\0'])
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(invalid("Unsafe ZIP member path"));
    }
    Ok(name.to_owned())
}

#[derive(Clone)]
struct Member {
    index: Option<usize>,
    directory: bool,
    bytes: u64,
}

fn open(path: &Path) -> io::Result<ZipArchive<File>> {
    validated(File::open(path)?)
}

fn validated(file: File) -> io::Result<ZipArchive<File>> {
    let mut reader = file.try_clone()?;
    let zip = ZipArchive::new(file)?;
    // ZipArchive deduplicates identical names. Count the original central records
    // so rebuilding cannot silently discard an earlier entry with the same name.
    reader.seek(SeekFrom::Start(zip.central_directory_start()))?;
    for _ in 0..zip.len() {
        let mut header = [0u8; 46];
        reader.read_exact(&mut header)?;
        if &header[..4] != b"PK\x01\x02" {
            return Err(invalid("Invalid ZIP central directory"));
        }
        let extra = [28, 30, 32]
            .iter()
            .map(|offset| u16::from_le_bytes([header[*offset], header[*offset + 1]]) as i64)
            .sum();
        reader.seek(SeekFrom::Current(extra))?;
    }
    let mut signature = [0u8; 4];
    reader.read_exact(&mut signature)?;
    if &signature == b"PK\x01\x02" {
        return Err(invalid("Duplicate ZIP member names"));
    }
    Ok(zip)
}

/// Include implicit folders and refuse ambiguous or escaping names.
fn inventory(zip: &mut ZipArchive<File>) -> io::Result<BTreeMap<PathBuf, Member>> {
    let mut members = BTreeMap::<PathBuf, Member>::new();
    let mut explicit = HashSet::new();
    for index in 0..zip.len() {
        let file = zip.by_index_raw(index)?;
        let name = file.name().trim_end_matches('/');
        let path = PathBuf::from(name);
        if name.is_empty() || member_name(&path)? != name || !explicit.insert(path.clone()) {
            return Err(invalid("Invalid or duplicate ZIP member name"));
        }
        if members
            .get(&path)
            .is_some_and(|member| member.directory != file.is_dir())
        {
            return Err(invalid("Conflicting ZIP file and folder names"));
        }
        members.insert(
            path.clone(),
            Member {
                index: Some(index),
                directory: file.is_dir(),
                bytes: file.size(),
            },
        );
        for parent in path
            .ancestors()
            .skip(1)
            .filter(|p| !p.as_os_str().is_empty())
        {
            if members.get(parent).is_some_and(|member| !member.directory) {
                return Err(invalid("A ZIP file is used as a folder"));
            }
            members.entry(parent.to_path_buf()).or_insert(Member {
                index: None,
                directory: true,
                bytes: 0,
            });
        }
    }
    Ok(members)
}

fn require_folder(members: &BTreeMap<PathBuf, Member>, path: &Path) -> io::Result<()> {
    member_name(path)?;
    if path.as_os_str().is_empty() || members.get(path).is_some_and(|member| member.directory) {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            "ZIP folder does not exist",
        ))
    }
}

pub fn read_directory(archive: &Path, folder: &Path, hidden: bool) -> io::Result<Vec<Entry>> {
    let members = inventory(&mut open(archive)?)?;
    require_folder(&members, folder)?;
    let mut entries = Vec::new();
    for (path, member) in members {
        if path.parent() != Some(folder) {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if !hidden && name.starts_with('.') {
            continue;
        }
        entries.push(Entry {
            path: archive.join(path),
            name,
            directory: member.directory,
            bytes: (!member.directory).then_some(member.bytes),
        });
    }
    entries.sort_by_cached_key(|entry| {
        (
            !entry.directory,
            entry.name.to_lowercase(),
            entry.path.clone(),
        )
    });
    Ok(entries)
}

pub fn entry(path: &Path) -> io::Result<Entry> {
    let (archive, member) = split(path).ok_or_else(|| invalid("Not a ZIP member"))?;
    let members = inventory(&mut open(&archive)?)?;
    let item = members
        .get(&member)
        .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
    Ok(Entry {
        path: path.to_path_buf(),
        name: member
            .file_name()
            .ok_or_else(|| invalid("No file name"))?
            .to_string_lossy()
            .into_owned(),
        directory: item.directory,
        bytes: (!item.directory).then_some(item.bytes),
    })
}

pub fn directory_size(archive: &Path, folder: &Path) -> io::Result<u64> {
    let members = inventory(&mut open(archive)?)?;
    require_folder(&members, folder)?;
    Ok(members
        .iter()
        .filter(|(path, member)| path.starts_with(folder) && !member.directory)
        .fold(0u64, |size, (_, member)| size.saturating_add(member.bytes)))
}

pub fn read_prefix(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    let (archive, member) = split(path).ok_or_else(|| invalid("Not a ZIP member"))?;
    let mut zip = open(&archive)?;
    let members = inventory(&mut zip)?;
    let item = members
        .get(&member)
        .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
    let file = zip.by_index(item.index.ok_or_else(|| invalid("Not a regular file"))?)?;
    if file.is_dir() || file.is_symlink() {
        return Err(invalid("Not a regular ZIP file"));
    }
    let mut bytes = Vec::new();
    file.take(limit).read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Kept alive by the preview or launcher; never extract to an archive-supplied path.
#[derive(Debug)]
pub struct Materialized {
    pub path: PathBuf,
    pub _directory: TempDir,
}

pub fn materialize(path: &Path, limit: u64) -> io::Result<Materialized> {
    let directory = tempfile::tempdir()?;
    let target = directory
        .path()
        .join(path.file_name().ok_or_else(|| invalid("No file name"))?);
    let (archive, member) = split(path).ok_or_else(|| invalid("Not a ZIP member"))?;
    let mut zip = open(&archive)?;
    let members = inventory(&mut zip)?;
    let item = members
        .get(&member)
        .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
    if item.directory || item.bytes > limit {
        return Err(invalid("ZIP file is too large or is not a regular file"));
    }
    let file = zip.by_index(item.index.ok_or_else(|| invalid("Not a regular file"))?)?;
    if file.is_dir() || file.is_symlink() {
        return Err(invalid("Not a regular ZIP file"));
    }
    let mut output = File::create(&target)?;
    if io::copy(&mut file.take(limit.saturating_add(1)), &mut output)? > limit {
        return Err(invalid("ZIP file is too large to preview"));
    }
    Ok(Materialized {
        path: target,
        _directory: directory,
    })
}

fn extract(
    zip: &mut ZipArchive<File>,
    members: &BTreeMap<PathBuf, Member>,
    source: &Path,
    target: &Path,
) -> io::Result<()> {
    let member = members
        .get(source)
        .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
    if member.directory {
        fs::create_dir(target)?;
        for path in members.keys().filter(|path| path.parent() == Some(source)) {
            extract(zip, members, path, &target.join(path.file_name().unwrap()))?;
        }
    } else {
        let mut file = zip.by_index(member.index.unwrap())?;
        if file.is_symlink()
            || file
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 != 0 && mode & 0o170000 != 0o100000)
        {
            return Err(invalid("ZIP links and special files cannot be extracted"));
        }
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)?;
        io::copy(&mut file, &mut output)?;
        #[cfg(unix)]
        if let Some(mode) = file.unix_mode() {
            output.set_permissions(fs::Permissions::from_mode(mode & 0o777))?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn signature(metadata: &fs::Metadata) -> (u64, u64, u64, i64, i64, i64, i64) {
    (
        metadata.dev(),
        metadata.ino(),
        metadata.len(),
        metadata.mtime(),
        metadata.mtime_nsec(),
        metadata.ctime(),
        metadata.ctime_nsec(),
    )
}

#[cfg(windows)]
fn signature(metadata: &fs::Metadata) -> (u64, u64, u64, i64, i64, i64, i64) {
    let secs = |time: std::io::Result<std::time::SystemTime>| {
        time.ok()
            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    };
    (
        metadata.len(),
        0,
        0,
        secs(metadata.modified()),
        0,
        secs(metadata.created()),
        0,
    )
}

fn rewrite(
    archive: &Path,
    expected: &fs::Metadata,
    mut names: impl FnMut(&Path) -> Vec<PathBuf>,
    additions: &[(PathBuf, PathBuf)],
) -> io::Result<()> {
    let archive = archive.canonicalize()?;
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&archive)?;
    let original = file.metadata()?;
    if original.permissions().readonly() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "ZIP archive is read-only",
        ));
    }
    if signature(&original) != signature(expected) {
        return Err(io::Error::other("ZIP changed during the operation; retry"));
    }
    let mut zip = validated(file)?;
    let members = inventory(&mut zip)?;
    // zip's raw-copy API does not retain encryption flags or special file types.
    // Refuse these archives rather than silently changing unrelated records.
    for index in 0..zip.len() {
        let file = zip.by_index_raw(index)?;
        if file.encrypted()
            || file.is_symlink()
            || file
                .unix_mode()
                .is_some_and(|mode| !matches!(mode & 0o170000, 0 | 0o100000 | 0o040000))
        {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "ZIP archives containing encrypted members, links or special files cannot be modified",
            ));
        }
    }
    let mut temporary = NamedTempFile::new_in(archive.parent().unwrap())?;
    {
        let mut writer = ZipWriter::new(temporary.as_file_mut());
        writer.set_raw_comment(zip.comment().into());
        for (path, member) in &members {
            for target in names(path) {
                let mut name = member_name(&target)?;
                if member.directory {
                    name.push('/');
                }
                if let Some(index) = member.index {
                    writer.raw_copy_file_rename(zip.by_index_raw(index)?, name)?;
                } else {
                    writer.add_directory(name, SimpleFileOptions::default())?;
                }
            }
        }
        for (source, target) in additions {
            append(&mut writer, source, target)?;
        }
        writer.finish()?;
    }
    temporary
        .as_file()
        .set_permissions(original.permissions())?;
    temporary.as_file().sync_all()?;
    if signature(&fs::metadata(&archive)?) != signature(&original) {
        return Err(io::Error::other("ZIP changed during the operation; retry"));
    }
    temporary.persist(&archive).map_err(|error| error.error)?;
    Ok(())
}

fn append<W: Write + io::Seek>(
    writer: &mut ZipWriter<W>,
    source: &Path,
    target: &Path,
) -> io::Result<()> {
    let metadata = fs::symlink_metadata(source)?;
    if metadata.is_symlink() || (!metadata.is_dir() && !metadata.is_file()) {
        return Err(invalid(
            "Links and special files cannot be added to ZIP archives",
        ));
    }
    #[cfg(unix)]
    let options =
        SimpleFileOptions::default().unix_permissions(metadata.permissions().mode() & 0o777);
    #[cfg(windows)]
    let options = SimpleFileOptions::default();
    let name = member_name(target)?;
    if metadata.is_dir() {
        writer.add_directory(format!("{name}/"), options)?;
        for item in fs::read_dir(source)? {
            let item = item?;
            append(writer, &item.path(), &target.join(item.file_name()))?;
        }
    } else {
        writer.start_file(name, options.large_file(metadata.len() >= u32::MAX as u64))?;
        io::copy(&mut File::open(source)?, writer)?;
    }
    Ok(())
}

pub fn rename(source: &Path, name: &str) -> io::Result<()> {
    let _guard = WRITE_LOCK
        .lock()
        .map_err(|_| io::Error::other("ZIP write lock unavailable"))?;
    let (archive, member) = split(source).ok_or_else(|| invalid("Not a ZIP member"))?;
    let target = super::operations::named_path(member.parent().unwrap(), name)?;
    let original = fs::metadata(&archive)?;
    let members = inventory(&mut open(&archive)?)?;
    if !members.contains_key(&member) {
        return Err(io::ErrorKind::NotFound.into());
    }
    if member == target {
        return Ok(());
    }
    if members.contains_key(&target) {
        return Err(io::ErrorKind::AlreadyExists.into());
    }
    rewrite(
        &archive,
        &original,
        |path| {
            vec![
                path.strip_prefix(&member)
                    .map(|rest| join(&target, rest))
                    .unwrap_or_else(|_| path.to_path_buf()),
            ]
        },
        &[],
    )
}

fn canonical_path(path: &Path) -> io::Result<PathBuf> {
    if let Some((archive, member)) =
        split(path).filter(|(_, member)| !member.as_os_str().is_empty())
    {
        member_name(&member)?;
        return Ok(join(&archive.canonicalize()?, &member));
    }
    Ok(path
        .parent()
        .unwrap_or(Path::new("."))
        .canonicalize()?
        .join(path.file_name().ok_or_else(|| invalid("No file name"))?))
}

pub fn transfer(sources: Vec<PathBuf>, directory: PathBuf, cut: bool) -> io::Result<()> {
    let _guard = WRITE_LOCK
        .lock()
        .map_err(|_| io::Error::other("ZIP write lock unavailable"))?;
    let directory = if let Some((archive, member)) = split(&directory) {
        join(&archive.canonicalize()?, &member)
    } else {
        directory.canonicalize()?
    };
    let destination_zip = split(&directory);
    let mut originals = BTreeMap::<PathBuf, fs::Metadata>::new();
    let destination_members = if let Some((archive, folder)) = &destination_zip {
        originals.insert(archive.clone(), fs::metadata(archive)?);
        let members = inventory(&mut open(archive)?)?;
        require_folder(&members, folder)?;
        Some(members)
    } else {
        if !directory.is_dir() {
            return Err(io::ErrorKind::NotADirectory.into());
        }
        None
    };
    let mut sources = sources
        .iter()
        .map(|source| canonical_path(source))
        .collect::<io::Result<Vec<_>>>()?;
    sources.sort();
    sources.dedup();
    // A selected parent carries its descendants, both on disk and in an archive.
    let selected = sources.clone();
    sources.retain(|source| {
        !selected
            .iter()
            .any(|parent| parent != source && source.starts_with(parent))
    });
    let mut targets = HashSet::new();
    let mut transfers = Vec::new();
    for source in sources {
        if directory.starts_with(&source) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Cannot transfer a folder or ZIP into itself",
            ));
        }
        let target = directory.join(source.file_name().unwrap());
        if cut && source == target {
            continue;
        }
        let exists = if let Some((archive, _)) = &destination_zip {
            destination_members
                .as_ref()
                .unwrap()
                .contains_key(target.strip_prefix(archive).unwrap())
        } else {
            fs::symlink_metadata(&target).is_ok()
        };
        if exists || !targets.insert(target.clone()) {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        transfers.push((source, target));
    }
    if transfers.is_empty() {
        return Ok(());
    }
    // Within one ZIP, copy compressed records directly and commit the entire move once.
    if let Some((archive, _)) = &destination_zip
        && transfers.iter().all(|(source, _)| {
            split(source)
                .is_some_and(|(zip, member)| zip == *archive && !member.as_os_str().is_empty())
        })
    {
        let mappings = transfers
            .iter()
            .map(|(source, target)| {
                (
                    source.strip_prefix(archive).unwrap(),
                    target.strip_prefix(archive).unwrap(),
                )
            })
            .collect::<Vec<_>>();
        for (source, _) in &mappings {
            if !destination_members.as_ref().unwrap().contains_key(*source) {
                return Err(io::ErrorKind::NotFound.into());
            }
        }
        return rewrite(
            archive,
            &originals[archive],
            |path| {
                if let Some((source, target)) =
                    mappings.iter().find(|(source, _)| path.starts_with(source))
                {
                    let moved = join(target, path.strip_prefix(source).unwrap());
                    if cut {
                        vec![moved]
                    } else {
                        vec![path.to_path_buf(), moved]
                    }
                } else {
                    vec![path.to_path_buf()]
                }
            },
            &[],
        );
    }
    // Stage every source before writing any destination. A failed extraction leaves originals intact.
    let staging = tempfile::tempdir()?;
    let mut additions = Vec::new();
    for (source, target) in &transfers {
        let staged = staging.path().join(source.file_name().unwrap());
        if let Some((archive, member)) =
            split(source).filter(|(_, member)| !member.as_os_str().is_empty())
        {
            if !originals.contains_key(&archive) {
                originals.insert(archive.clone(), fs::metadata(&archive)?);
            }
            let mut zip = open(&archive)?;
            let members = inventory(&mut zip)?;
            extract(&mut zip, &members, &member, &staged)?;
        } else {
            super::operations::copy(source, &staged)?;
        }
        additions.push((staged, target.clone()));
    }
    for (archive, original) in &originals {
        if signature(&fs::metadata(archive)?) != signature(original) {
            return Err(io::Error::other("ZIP changed during the operation; retry"));
        }
    }
    if let Some((archive, _)) = &destination_zip {
        let additions = additions
            .iter()
            .map(|(source, target)| {
                (
                    source.clone(),
                    target.strip_prefix(archive).unwrap().to_path_buf(),
                )
            })
            .collect::<Vec<_>>();
        rewrite(
            archive,
            &originals[archive],
            |path| vec![path.to_path_buf()],
            &additions,
        )?;
        originals.insert(archive.clone(), fs::metadata(archive)?);
    } else {
        super::operations::transfer(
            additions.iter().map(|(source, _)| source.clone()).collect(),
            directory,
            false,
        )?;
    }
    if cut {
        let mut removals = BTreeMap::<PathBuf, Vec<PathBuf>>::new();
        for (source, _) in transfers {
            if let Some((archive, member)) =
                split(&source).filter(|(_, member)| !member.as_os_str().is_empty())
            {
                removals.entry(archive).or_default().push(member);
            } else {
                super::operations::remove(&source)?;
            }
        }
        for (archive, removed) in removals {
            rewrite(
                &archive,
                &originals[&archive],
                |path| {
                    if removed.iter().any(|source| path.starts_with(source)) {
                        vec![]
                    } else {
                        vec![path.to_path_buf()]
                    }
                },
                &[],
            )?;
        }
    }
    Ok(())
}

pub fn create(directory: &Path, name: &str, folder: bool) -> io::Result<()> {
    let staging = tempfile::tempdir()?;
    let path = super::operations::named_path(staging.path(), name)?;
    if folder {
        fs::create_dir(&path)?;
    } else {
        File::create(&path)?;
    }
    transfer(vec![path], directory.to_path_buf(), false)
}

#[cfg(all(test, unix))]
#[path = "../../tests/infrastructure/archive.rs"]
mod tests;
