//! Open a path with whatever the host system uses to hand a file to the
//! application that owns it.
//!
//! The caller only asks for "open this"; picking the mechanism belongs to the
//! platform, so nothing outside this module names `xdg-open`, `explorer.exe`
//! or `open`.

use std::path::Path;

/// Hand `path` to the desktop the way the host expects, waiting for the
/// launcher to report that it accepted the request.
///
/// Folders and files take the same route: the desktop decides from the path,
/// so a directory opens in the file manager and a document in its application.
pub(crate) fn open(path: &Path) -> std::io::Result<()> {
    launcher().run(path)
}

/// The command that opens a path on this host.
struct Launcher {
    program: &'static str,
    /// Arguments placed before the path.
    leading: &'static [&'static str],
    /// When true the launcher must not be waited for: on Windows `explorer`
    /// would then hold the process open for as long as the window stays.
    detached: bool,
}

/// The launcher program this build will run, exposed so the test can check it
/// against `PATH` instead of trusting the constant.
#[cfg(test)]
pub(crate) const fn launcher_program() -> &'static str {
    launcher().program
}

impl Launcher {
    fn run(&self, path: &Path) -> std::io::Result<()> {
        let mut command = std::process::Command::new(self.program);
        command.args(self.leading).arg(path);
        if !self.detached {
            // Wait for the launcher so a failure is reported to the user.
            return command.status().map(|_| ());
        }
        // `explorer.exe` would keep the manager's console open for as long as
        // the window is displayed, and flashes one when spawned normally.
        // `DETACHED_PROCESS` and `CREATE_NO_WINDOW` keep the launch invisible,
        // and not waiting is what keeps the manager responsive.
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const DETACHED_PROCESS: u32 = 0x0000_0008;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW);
        }
        command.spawn().map(|_| ())
    }
}

#[cfg(target_os = "linux")]
const fn launcher() -> Launcher {
    Launcher {
        program: "xdg-open",
        leading: &[],
        detached: false,
    }
}

#[cfg(target_os = "macos")]
const fn launcher() -> Launcher {
    Launcher {
        program: "open",
        leading: &[],
        detached: false,
    }
}

#[cfg(windows)]
const fn launcher() -> Launcher {
    Launcher {
        program: "explorer.exe",
        leading: &[],
        detached: true,
    }
}

#[cfg(test)]
#[path = "../../tests/platform/shell_open.rs"]
mod shell_open_tests;
