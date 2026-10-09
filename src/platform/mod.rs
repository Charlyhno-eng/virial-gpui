//! Platform backends selected at compile time. Linux keeps the original
//! implementations; other targets use the portable fallbacks next to them.
//! Call sites use the re-exported names (`platform::places`, ...), never a
//! backend module directly.
#[cfg(target_os = "linux")]
pub(crate) mod linux;
#[cfg(windows)]
pub(crate) mod windows;
#[cfg(target_os = "linux")]
pub(crate) use linux::{applications, devices, file_drag, places};

#[cfg(not(target_os = "linux"))]
pub(crate) mod applications;
#[cfg(not(target_os = "linux"))]
pub(crate) mod desktop;
#[cfg(not(target_os = "linux"))]
pub(crate) mod devices;
#[cfg(not(target_os = "linux"))]
pub(crate) mod file_drag;
#[cfg(not(target_os = "linux"))]
pub(crate) mod places;

#[cfg(test)]
#[path = "../../tests/platform/storage.rs"]
mod portable_storage_tests;

#[cfg(test)]
#[path = "../../tests/platform/portable.rs"]
mod portable_platform_tests;
