mod app;
mod config;
mod domain;
mod infrastructure;
mod platform;
mod state;
mod ui;

use app::FileManager;
use gpui::{
    App, Application, Bounds, WindowBackgroundAppearance, WindowBounds, WindowDecorations,
    WindowOptions, prelude::*, px, size,
};

fn main() {
    if let Err(error) = platform::linux::desktop::register() {
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
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    initial_size,
                    cx,
                ))),
                titlebar: None,
                window_decorations: Some(WindowDecorations::Client),
                window_background: WindowBackgroundAppearance::Transparent,
                app_id: Some(platform::linux::desktop::APP_ID.into()),
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
