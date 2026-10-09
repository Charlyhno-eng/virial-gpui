//! Portable device backends. Windows enumerates volumes with the drive API;
//! other non-Linux targets keep the "no devices" fallback with clean errors.
use std::{io, path::PathBuf};

#[cfg(not(windows))]
use futures_lite::future;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Volume {
    pub object: String,
    pub drive: String,
    pub device: PathBuf,
    pub label: String,
    pub size: u64,
    pub mountpoints: Vec<PathBuf>,
    pub can_power_off: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Open,
    Unmount,
    SafelyRemove,
}

#[cfg(windows)]
mod backend {
    use super::Volume;
    use std::{io, path::PathBuf};
    use windows_sys::Win32::Storage::FileSystem::{
        GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDriveStringsW, GetVolumeInformationW,
    };
    // GetDriveTypeW return values, from the Win32 drive-type table.
    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;
    const DRIVE_CDROM: u32 = 5;

    pub(crate) fn discover() -> io::Result<Vec<Volume>> {
        let needed = unsafe { GetLogicalDriveStringsW(0, std::ptr::null_mut()) };
        if needed == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut buffer = vec![0u16; needed as usize + 1];
        let written = unsafe { GetLogicalDriveStringsW(buffer.len() as u32, buffer.as_mut_ptr()) };
        if written == 0 || written as usize > buffer.len() {
            return Err(io::Error::last_os_error());
        }
        // The buffer is a NUL-separated list: "C:\\\0D:\\\0\0".
        let roots = String::from_utf16_lossy(&buffer[..written as usize]);
        let mut volumes = Vec::new();
        for root in roots.split('\0').filter(|root| !root.is_empty()) {
            let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
            let kind = unsafe { GetDriveTypeW(wide.as_ptr()) };
            if kind != DRIVE_FIXED && kind != DRIVE_REMOVABLE && kind != DRIVE_CDROM {
                // Network shares and RAM disks are not sidebar material.
                continue;
            }
            let mut label = [0u16; 261];
            let mut serial = 0u32;
            let mut length = 0u32;
            let mut flags = 0u32;
            let mut filesystem = [0u16; 64];
            let named = unsafe {
                GetVolumeInformationW(
                    wide.as_ptr(),
                    label.as_mut_ptr(),
                    label.len() as u32,
                    &mut serial,
                    &mut length,
                    &mut flags,
                    filesystem.as_mut_ptr(),
                    filesystem.len() as u32,
                )
            };
            let label = if named != 0 {
                let end = label.iter().position(|&unit| unit == 0).unwrap_or(0);
                String::from_utf16_lossy(&label[..end])
            } else {
                String::new()
            };
            let mut total = 0u64;
            let mut free = 0u64;
            let sized = unsafe {
                GetDiskFreeSpaceExW(wide.as_ptr(), std::ptr::null_mut(), &mut total, &mut free)
            };
            volumes.push(Volume {
                object: root.to_string(),
                drive: root.to_string(),
                device: PathBuf::from(root),
                label: if label.is_empty() {
                    format!("{} Drive", root.trim_end_matches('\\'))
                } else {
                    label
                },
                size: if sized != 0 { total } else { 0 },
                mountpoints: vec![PathBuf::from(root)],
                // Eject needs CM_Request_Device_Eject; the flag stays hidden
                // until implemented so the sidebar never offers a button that
                // only errors.
                can_power_off: false,
            });
        }
        Ok(volumes)
    }

    /// No device-change subscription yet: poll, so plugging a USB key shows up
    /// within one refresh interval.
    pub(crate) async fn changed() -> io::Result<()> {
        std::thread::sleep(std::time::Duration::from_secs(2));
        Ok(())
    }
}

#[cfg(windows)]
pub(crate) fn discover() -> io::Result<Vec<Volume>> {
    backend::discover()
}

#[cfg(windows)]
pub(crate) struct Monitor;

#[cfg(windows)]
impl Monitor {
    pub(crate) async fn new() -> io::Result<Self> {
        Ok(Monitor)
    }

    pub(crate) async fn discover(&mut self) -> io::Result<Vec<Volume>> {
        backend::discover()
    }

    pub(crate) async fn changed(&mut self) -> io::Result<()> {
        backend::changed().await
    }
}

#[cfg(not(windows))]
pub(crate) fn discover() -> io::Result<Vec<Volume>> {
    Ok(Vec::new())
}

#[cfg(not(windows))]
pub(crate) struct Monitor;

#[cfg(not(windows))]
impl Monitor {
    pub(crate) async fn new() -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "No volume monitor on this platform yet",
        ))
    }

    pub(crate) async fn discover(&mut self) -> io::Result<Vec<Volume>> {
        discover()
    }

    pub(crate) async fn changed(&mut self) -> io::Result<()> {
        future::pending::<()>().await;
        Ok(())
    }
}

pub(crate) fn execute(_volume: &Volume, _action: Action) -> io::Result<Option<PathBuf>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Ejecting volumes is not supported on this platform yet",
    ))
}
