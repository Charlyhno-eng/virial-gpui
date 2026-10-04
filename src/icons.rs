//! Original SVG icons embedded in the binary; no runtime asset directory is needed.
use crate::theme;
use gpui::{AssetSource, Result, SharedString, Svg, prelude::*, px, svg};
use std::borrow::Cow;

pub struct IconAssets;

const ASSETS: &[(&str, &[u8])] = &[
    (
        "icons/fullscreen.svg",
        include_bytes!("../assets/icons/fullscreen.svg"),
    ),
    (
        "virial-gpui-logo.png",
        include_bytes!("../assets/virial-gpui-logo.png"),
    ),
    (
        "icons/minimize.svg",
        include_bytes!("../assets/icons/minimize.svg"),
    ),
    (
        "icons/maximize.svg",
        include_bytes!("../assets/icons/maximize.svg"),
    ),
    (
        "icons/restore.svg",
        include_bytes!("../assets/icons/restore.svg"),
    ),
    (
        "icons/close.svg",
        include_bytes!("../assets/icons/close.svg"),
    ),
    (
        "icons/network.svg",
        include_bytes!("../assets/icons/network.svg"),
    ),
    (
        "icons/recent.svg",
        include_bytes!("../assets/icons/recent.svg"),
    ),
    (
        "icons/folder.svg",
        include_bytes!("../assets/icons/folder.svg"),
    ),
    ("icons/file.svg", include_bytes!("../assets/icons/file.svg")),
    ("icons/home.svg", include_bytes!("../assets/icons/home.svg")),
    (
        "icons/drive.svg",
        include_bytes!("../assets/icons/drive.svg"),
    ),
    ("icons/back.svg", include_bytes!("../assets/icons/back.svg")),
    (
        "icons/forward.svg",
        include_bytes!("../assets/icons/forward.svg"),
    ),
    ("icons/up.svg", include_bytes!("../assets/icons/up.svg")),
    (
        "icons/refresh.svg",
        include_bytes!("../assets/icons/refresh.svg"),
    ),
    ("icons/eye.svg", include_bytes!("../assets/icons/eye.svg")),
    ("icons/open.svg", include_bytes!("../assets/icons/open.svg")),
    (
        "icons/download.svg",
        include_bytes!("../assets/icons/download.svg"),
    ),
    (
        "icons/image.svg",
        include_bytes!("../assets/icons/image.svg"),
    ),
    (
        "icons/music.svg",
        include_bytes!("../assets/icons/music.svg"),
    ),
    ("icons/code.svg", include_bytes!("../assets/icons/code.svg")),
    (
        "icons/archive.svg",
        include_bytes!("../assets/icons/archive.svg"),
    ),
];

impl AssetSource for IconAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(ASSETS
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(ASSETS
            .iter()
            .filter(|(name, _)| name.starts_with(path))
            .map(|(name, _)| (*name).into())
            .collect())
    }
}

pub fn icon(name: &str, size: f32, color: u32) -> Svg {
    svg()
        .path(format!("icons/{name}.svg"))
        .size(px(size))
        .flex_shrink_0()
        .text_color(theme::color(color))
}
