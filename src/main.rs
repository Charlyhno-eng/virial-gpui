// Hide the console window on Windows so launching `virial-gpui.exe` from the
// installer, the desktop shortcut or `Win+R` never spawns an extra terminal
// behind the GUI window. The flag is a no-op on Linux/macOS and stays out
// of the way for the `cargo run` development loop.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod config;
mod domain;
mod infrastructure;
mod platform;
mod state;
mod ui;

use app::FileManager;
use gpui::{
    App, Application, Bounds, WindowBackgroundAppearance, WindowDecorations, WindowOptions,
    prelude::*, px, size,
};

/// AppUserModelID used to group Virial windows on the Windows taskbar. The
/// value is owned by the platform-specific module so the portable fallback
/// (which is a no-op) does not need to know about Windows-specific names.
#[cfg(target_os = "linux")]
const APP_ID: &str = platform::linux::desktop::APP_ID;
#[cfg(windows)]
const APP_ID: &str = platform::windows::desktop::APP_ID;
#[cfg(not(any(target_os = "linux", windows)))]
const APP_ID: &str = "virial-gpui";

fn main() {
    // Register downloaded binaries without requiring a graphical session.
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--install-desktop")) {
        if let Err(error) = install_desktop() {
            eprintln!("Cannot register Virial's desktop icon: {error}");
            std::process::exit(1);
        }
        return;
    }
    if let Err(error) = install_desktop() {
        eprintln!("Cannot register Virial's desktop icon: {error}");
    }
    let path = config::initial_path();
    Application::new()
        .with_assets(ui::icons::IconAssets)
        .run(move |cx: &mut App| {
            cx.on_window_closed(|cx| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            let minimum_size = size(px(720.), px(420.));
            let preferred_size = size(px(900.), px(580.));
            let initial_size = cx
                .primary_display()
                .map(|display| {
                    let available = display.bounds().size;
                    let compact = size(available.width * 0.82, available.height * 0.82);
                    preferred_size.min(&compact.max(&minimum_size))
                })
                .unwrap_or(preferred_size);
            let options = WindowOptions {
                window_bounds: Some(
                    crate::infrastructure::layout::Layout::load()
                        .bounds(Bounds::centered(None, initial_size, cx), cx),
                ),
                titlebar: None,
                window_decorations: Some(WindowDecorations::Client),
                window_background: WindowBackgroundAppearance::Transparent,
                // Match the desktop launcher for window icons. GPUI 0.2.2
                // panics when requesting a raw X11 window handle.
                app_id: Some(APP_ID.into()),
                window_min_size: Some(minimum_size),
                ..Default::default()
            };
            if let Err(error) = cx.open_window(options, |window, cx| {
                window.set_rem_size(px(14.));
                cx.new(|cx| FileManager::new(path, window, cx))
            }) {
                eprintln!(
                    "{}: {error}",
                    ui::i18n::Language::system().text("Cannot open Virial")
                );
                cx.quit();
            }
            cx.activate(true);
        });
}

/// Dispatch to the right platform-specific desktop installer. The Linux
/// side writes a `.desktop` file under `$XDG_DATA_HOME`; the Windows side
/// sets the AppUserModelID and (only with `--install-desktop`) registers
/// the file associations declared in [`platform::windows::desktop`].
fn install_desktop() -> std::io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        platform::linux::desktop::register()
    }
    #[cfg(windows)]
    {
        platform::windows::desktop::register()
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        platform::desktop::register()
    }
}
