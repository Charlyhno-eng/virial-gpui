//! Windows-specific desktop identity and file-association registration.
//!
//! The portable target ([`super::fallback`]) only sets an AppUserModelID so
//! that Windows groups the running window under a stable identity on the
//! taskbar instead of falling back to the generic "rustc.exe" host. The
//! `--install-desktop` flag goes further and asks the current user to
//! claim a set of plain-text file extensions (`.env`, `.gitignore`,
//! `.toml`, `.lock`, `.cfg`, `.ini`) under the Virial icon: the keys are
//! written under `HKCU\Software\Classes` so no admin rights are needed,
//! and removing them is one `reg delete` away.
//!
//! `register()` is intentionally non-fatal: the binary still launches if
//! the registry is read-only, it just falls back to the unbranded icon.

#![cfg(windows)]

use std::{
    env,
    ffi::OsStr,
    io,
    path::{Path, PathBuf},
    process::Command,
};

use windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;

/// Application User Model ID used to group Virial windows on the taskbar.
///
/// The literal string must match the `ApplicationUserModelID` declared in
/// the .desktop launcher on Linux (and the AppUserModelID that future
/// installers will write under `HKCR\Applications\virial-gpui.exe`) so a
/// single brand identity spans every desktop we ship to.
pub(crate) const APP_ID: &str = "DeamonDev888.Virial.GPUI.1";

/// Full human-readable application name surfaced in shell dialogs.
pub(crate) const APP_NAME: &str = "Virial";

/// File extensions Virial can preview natively. Each one becomes an
/// `OpenWithProgids` advertisement pointing at our ProgID plus a
/// `DefaultIcon` so the Explorer icon reflects the brand even before
/// the user picks Virial as the default opener.
const PREVIEWED_EXTENSIONS: &[&str] = &[
    "env",
    "gitignore",
    "toml",
    "lock",
    "cfg",
    "ini",
    "properties",
];

/// ProgID registered for the bundled icon. Different from `APP_ID`
/// because ProgIDs are file-extension namespaces while AppUserModelIDs
/// are taskbar/launch grouping identifiers.
const PROG_ID: &str = "Virial.GPUI.TextFile";

pub(crate) fn register() -> io::Result<()> {
    // 1. Always claim the AppUserModelID, even outside the install
    //    flow, so that ad-hoc launches from a terminal still attach to
    //    the proper taskbar group.
    set_app_user_model_id()?;

    // 2. The `--install-desktop` flag is the only path that writes to
    //    the registry; every other launch is read-only.
    let wants_install =
        env::args_os().nth(1).as_deref().map(OsStr::new) == Some(OsStr::new("--install-desktop"));
    if wants_install {
        install_file_associations()?;
    }
    Ok(())
}

fn set_app_user_model_id() -> io::Result<()> {
    // SHGetFolderPathW + SetCurrentProcessExplicitAppUserModelID live
    // in shell32.dll. We bind them at runtime so a missing shell32 on
    // an unusual Windows SKU does not stop the launch.
    let wide: Vec<u16> = APP_ID.encode_utf16().chain(std::iter::once(0)).collect();
    let result = unsafe { SetCurrentProcessExplicitAppUserModelID(wide.as_ptr()) };
    if result.is_err() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "SetCurrentProcessExplicitAppUserModelID failed",
        ));
    }
    Ok(())
}

/// Write the Virial ProgID plus `OpenWithProgids`/`DefaultIcon` keys
/// for every extension in [`PREVIEWED_EXTENSIONS`] under HKCU. Returns
/// the first I/O error encountered, if any.
fn install_file_associations() -> io::Result<()> {
    let binary = env::current_exe()?;
    let icon_ref = format!("\"{}\",0", binary.display());

    // ProgID + DefaultIcon ------------------------------------------------
    let progid_root = format!(r"HKCU\Software\Classes\{PROG_ID}");
    run_reg(&["add", &progid_root, "/ve", "/d", APP_NAME, "/f"])?;
    run_reg(&[
        "add",
        &format!(r"{progid_root}\DefaultIcon"),
        "/ve",
        "/d",
        &icon_ref,
        "/f",
    ])?;
    // A ProgID needs at least one shell\open\command verb to be valid.
    run_reg(&[
        "add",
        &format!(r"{progid_root}\shell\open\command"),
        "/ve",
        "/d",
        &format!("\"{}\" \"%1\"", binary.display()),
        "/f",
    ])?;

    // Extensions ----------------------------------------------------------
    for ext in PREVIEWED_EXTENSIONS {
        let ext_root = format!(r"HKCU\Software\Classes\.{ext}");
        run_reg(&["add", &ext_root, "/ve", "/d", PROG_ID, "/f"])?;
        run_reg(&[
            "add",
            &format!(
                r"HKCU\Software\Classes\Applications\{}\shell\open\command",
                binary.display()
            ),
            "/ve",
            "/d",
            &format!("\"{}\" \"%1\"", binary.display()),
            "/f",
        ])?;
        // Allow the user to pick Virial via "Open with..." even when
        // another handler is the default.
        run_reg(&[
            "add",
            &format!(r"HKCU\Software\Classes\.{ext}\OpenWithProgids"),
            "/v",
            PROG_ID,
            "/d",
            "",
            "/f",
        ])?;
    }

    // Rescan shell icons (best effort; failure is not fatal because the
    // new keys are already persisted and the Explorer icon cache will
    // refresh on its own within a few minutes).
    refresh_shell_icon_cache();
    Ok(())
}

fn run_reg(args: &[&str]) -> io::Result<()> {
    let output = Command::new("reg").args(args).output()?;
    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "reg {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ));
    }
    Ok(())
}

fn refresh_shell_icon_cache() {
    // `ie4uinit.exe -ClearIconCache` is the documented way to nudge the
    // shell into dropping its icon cache. Some SKUs do not ship it; the
    // helper simply returns silently.
    let _ = Command::new("ie4uinit.exe").arg("-ClearIconCache").output();
}

pub(crate) fn uninstall() -> io::Result<()> {
    let progid_root = format!(r"HKCU\Software\Classes\{PROG_ID}");
    run_reg(&["delete", &progid_root, "/f"])?;
    for ext in PREVIEWED_EXTENSIONS {
        let ext_root = format!(r"HKCU\Software\Classes\.{ext}\OpenWithProgids");
        run_reg(&["delete", &ext_root, "/v", PROG_ID, "/f"])?;
    }
    Ok(())
}

/// Locate the directory that holds the running executable, falling back
/// to the current working directory. Used by tests that need a writable
/// scratch path next to the binary layout.
#[allow(dead_code)]
pub(crate) fn exe_dir() -> io::Result<PathBuf> {
    let exe = env::current_exe()?;
    Ok(exe
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(".")))
}

/// Best-effort removal of the optional desktop shortcut we may have
/// dropped on a previous run. We never write the shortcut from
/// `--install-desktop` (that belongs to an installer); this helper
/// exists so the test suite can clean up after itself.
#[allow(dead_code)]
pub(crate) fn remove_desktop_shortcut() -> io::Result<()> {
    if let Some(profile) = env::var_os("USERPROFILE") {
        let shortcut = PathBuf::from(profile).join("Desktop").join("Virial.lnk");
        if shortcut.exists() {
            fs::remove_file(shortcut)?;
        }
    }
    Ok(())
}
