//! Mounted network locations from GVFS and Linux mountinfo; no network discovery.
use crate::files::{Entry, read_directory};
use std::{
    collections::HashSet,
    fs, io,
    os::unix::ffi::OsStringExt,
    path::{Path, PathBuf},
};

pub fn runtime_home() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", unsafe { libc::geteuid() })))
}

fn decode_mount_path(value: &str) -> PathBuf {
    let bytes = value.as_bytes();
    let mut decoded = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\'
            && index + 3 < bytes.len()
            && bytes[index + 1..index + 4]
                .iter()
                .all(|byte| (b'0'..=b'7').contains(byte))
        {
            let value = (bytes[index + 1] - b'0') as u16 * 64
                + (bytes[index + 2] - b'0') as u16 * 8
                + (bytes[index + 3] - b'0') as u16;
            decoded.push(value as u8);
            index += 4;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    std::ffi::OsString::from_vec(decoded).into()
}

fn mount_paths(text: &str) -> Vec<PathBuf> {
    text.lines()
        .filter_map(|line| {
            let (before, after) = line.split_once(" - ")?;
            let filesystem = after.split_whitespace().next()?;
            if !matches!(
                filesystem,
                "nfs"
                    | "nfs4"
                    | "cifs"
                    | "smb3"
                    | "fuse.sshfs"
                    | "fuse.davfs"
                    | "davfs"
                    | "fuse.rclone"
            ) {
                return None;
            }
            Some(decode_mount_path(before.split_whitespace().nth(4)?))
        })
        .collect()
}

pub fn read(runtime: &Path) -> io::Result<Vec<Entry>> {
    read_from(runtime, &fs::read_to_string("/proc/self/mountinfo")?)
}

fn read_from(runtime: &Path, mountinfo: &str) -> io::Result<Vec<Entry>> {
    let mut entries = match read_directory(&runtime.join("gvfs"), false) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
        result => result?,
    };
    entries.retain(|entry| {
        entry.directory && !entry.name.starts_with("mtp:") && !entry.name.starts_with("gphoto2:")
    });
    let mut seen: HashSet<_> = entries.iter().map(|entry| entry.path.clone()).collect();
    for path in mount_paths(mountinfo) {
        if !seen.insert(path.clone()) {
            continue;
        }
        if !fs::metadata(&path).is_ok_and(|metadata| metadata.is_dir()) {
            continue;
        }
        entries.push(Entry {
            name: path
                .file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy()
                .into_owned(),
            path,
            directory: true,
            bytes: None,
        });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.path.cmp(&b.path)));
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovers_gvfs_and_network_mounts_but_not_local_disks() {
        let root = std::env::temp_dir().join(format!("virial-network-test-{}", std::process::id()));
        fs::create_dir_all(root.join("gvfs/smb-share:server=nas,share=docs")).unwrap();
        fs::write(root.join("gvfs/not-a-share"), "").unwrap();
        let mount = root.join("shared files");
        fs::create_dir(&mount).unwrap();
        let escaped = mount.to_string_lossy().replace(' ', "\\040");
        let mounts = format!(
            "1 2 0:1 / {escaped} rw - cifs //nas/docs rw\n2 3 0:2 / / rw - ext4 /dev/sda rw\n3 4 0:3 / {escaped} rw - nfs nas:/docs rw"
        );
        assert_eq!(mount_paths(&mounts), vec![mount.clone(), mount.clone()]);
        let entries = read_from(&root, &mounts).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|entry| entry.path == mount));
        assert!(read_from(&root.join("absent"), "").unwrap().is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}
