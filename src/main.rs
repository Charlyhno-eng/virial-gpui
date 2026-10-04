mod actions;
mod app;
mod applications;
mod files;
mod i18n;
mod icons;
mod location;
mod navigation;
mod network;
mod operations;
mod places;
mod recent;
mod theme;
mod ui;

use app::FileManager;
use gpui::{
    App, Application, Bounds, WindowBounds, WindowDecorations, WindowOptions, prelude::*, px, size,
};
use std::path::PathBuf;

fn main() {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("/"));
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(path)
    };
    let path = path.canonicalize().unwrap_or(path);
    Application::new()
        .with_assets(icons::IconAssets)
        .run(move |cx: &mut App| {
            cx.on_window_closed(|cx| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1120.), px(760.)),
                    cx,
                ))),
                titlebar: None,
                window_decorations: Some(WindowDecorations::Client),
                app_id: Some("virial-gpui".into()),
                window_min_size: Some(size(px(800.), px(480.))),
                ..Default::default()
            };
            if let Err(error) = cx.open_window(options, |window, cx| {
                cx.new(|cx| FileManager::new(path, window, cx))
            }) {
                eprintln!(
                    "{}: {error}",
                    i18n::Language::system().text("Cannot open Virial")
                );
                cx.quit();
            }
            cx.activate(true);
        });
}
