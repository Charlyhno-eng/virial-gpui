//! SVG interface icons and upstream language logos embedded in the binary.
use crate::ui::theme;
use gpui::{AnyElement, AssetSource, Result, SharedString, Svg, img, prelude::*, px, svg};
use std::borrow::Cow;

pub struct IconAssets;

const ASSETS: &[(&str, &[u8])] = &[
    (
        "icons/file-outline.svg",
        include_bytes!("../../assets/icons/file-outline.svg"),
    ),
    (
        "icons/image-outline.svg",
        include_bytes!("../../assets/icons/image-outline.svg"),
    ),
    (
        "icons/music-outline.svg",
        include_bytes!("../../assets/icons/music-outline.svg"),
    ),
    (
        "icons/video-outline.svg",
        include_bytes!("../../assets/icons/video-outline.svg"),
    ),
    (
        "icons/image-convert.svg",
        include_bytes!("../../assets/icons/image-convert.svg"),
    ),
    (
        "icons/background-remove.svg",
        include_bytes!("../../assets/icons/background-remove.svg"),
    ),
    (
        "icons/trash.svg",
        include_bytes!("../../assets/icons/trash.svg"),
    ),
    (
        "icons/eject.svg",
        include_bytes!("../../assets/icons/eject.svg"),
    ),
    (
        "icons/down.svg",
        include_bytes!("../../assets/icons/down.svg"),
    ),
    (
        "icons/search.svg",
        include_bytes!("../../assets/icons/search.svg"),
    ),
    (
        "icons/info.svg",
        include_bytes!("../../assets/icons/info.svg"),
    ),
    (
        "icons/view.svg",
        include_bytes!("../../assets/icons/view.svg"),
    ),
    (
        "icons/fullscreen.svg",
        include_bytes!("../../assets/icons/fullscreen.svg"),
    ),
    (
        "icons/minimize.svg",
        include_bytes!("../../assets/icons/minimize.svg"),
    ),
    (
        "icons/maximize.svg",
        include_bytes!("../../assets/icons/maximize.svg"),
    ),
    (
        "icons/restore.svg",
        include_bytes!("../../assets/icons/restore.svg"),
    ),
    (
        "icons/copy.svg",
        include_bytes!("../../assets/icons/copy.svg"),
    ),
    (
        "icons/cut.svg",
        include_bytes!("../../assets/icons/cut.svg"),
    ),
    (
        "icons/paste.svg",
        include_bytes!("../../assets/icons/paste.svg"),
    ),
    (
        "icons/edit.svg",
        include_bytes!("../../assets/icons/edit.svg"),
    ),
    (
        "icons/folder-plus.svg",
        include_bytes!("../../assets/icons/folder-plus.svg"),
    ),
    (
        "icons/close.svg",
        include_bytes!("../../assets/icons/close.svg"),
    ),
    (
        "icons/recent.svg",
        include_bytes!("../../assets/icons/recent.svg"),
    ),
    (
        "icons/folder.svg",
        include_bytes!("../../assets/icons/folder.svg"),
    ),
    (
        "icons/file.svg",
        include_bytes!("../../assets/icons/file.svg"),
    ),
    (
        "icons/home.svg",
        include_bytes!("../../assets/icons/home.svg"),
    ),
    (
        "icons/drive.svg",
        include_bytes!("../../assets/icons/drive.svg"),
    ),
    (
        "icons/back.svg",
        include_bytes!("../../assets/icons/back.svg"),
    ),
    (
        "icons/forward.svg",
        include_bytes!("../../assets/icons/forward.svg"),
    ),
    ("icons/up.svg", include_bytes!("../../assets/icons/up.svg")),
    (
        "icons/refresh.svg",
        include_bytes!("../../assets/icons/refresh.svg"),
    ),
    (
        "icons/eye.svg",
        include_bytes!("../../assets/icons/eye.svg"),
    ),
    (
        "icons/open.svg",
        include_bytes!("../../assets/icons/open.svg"),
    ),
    (
        "icons/download.svg",
        include_bytes!("../../assets/icons/download.svg"),
    ),
    (
        "icons/image.svg",
        include_bytes!("../../assets/icons/image.svg"),
    ),
    (
        "icons/music.svg",
        include_bytes!("../../assets/icons/music.svg"),
    ),
    (
        "icons/video.svg",
        include_bytes!("../../assets/icons/video.svg"),
    ),
    (
        "icons/code.svg",
        include_bytes!("../../assets/icons/code.svg"),
    ),
    (
        "icons/archive.svg",
        include_bytes!("../../assets/icons/archive.svg"),
    ),
    (
        "icons/python.svg",
        include_bytes!("../../assets/icons/python.svg"),
    ),
    (
        "icons/javascript.svg",
        include_bytes!("../../assets/icons/javascript.svg"),
    ),
    (
        "icons/typescript.svg",
        include_bytes!("../../assets/icons/typescript.svg"),
    ),
    (
        "icons/react.svg",
        include_bytes!("../../assets/icons/react.svg"),
    ),
    (
        "icons/rust.svg",
        include_bytes!("../../assets/icons/rust.svg"),
    ),
    (
        "icons/html.svg",
        include_bytes!("../../assets/icons/html.svg"),
    ),
    (
        "icons/css.svg",
        include_bytes!("../../assets/icons/css.svg"),
    ),
    (
        "icons/sass.svg",
        include_bytes!("../../assets/icons/sass.svg"),
    ),
    (
        "icons/json.svg",
        include_bytes!("../../assets/icons/json.svg"),
    ),
    (
        "icons/config.svg",
        include_bytes!("../../assets/icons/config.svg"),
    ),
    (
        "icons/shell.svg",
        include_bytes!("../../assets/icons/shell.svg"),
    ),
    ("icons/c.svg", include_bytes!("../../assets/icons/c.svg")),
    (
        "icons/cpp.svg",
        include_bytes!("../../assets/icons/cpp.svg"),
    ),
    (
        "icons/pdf.svg",
        include_bytes!("../../assets/icons/pdf.svg"),
    ),
    (
        "icons/markdown.svg",
        include_bytes!("../../assets/icons/markdown.svg"),
    ),
    (
        "icons/remote.svg",
        include_bytes!("../../assets/icons/remote.svg"),
    ),
    (
        "icons/docker.svg",
        include_bytes!("../../assets/icons/docker.svg"),
    ),
    ("icons/git.svg", include_bytes!("../../assets/icons/git.svg")),
];

/// Whether the named icon is embedded in the binary. Lets callers (and the
/// tests) fail loudly instead of rendering an empty cell for a typo.
///
/// Only referenced from tests today; the loader itself falls back gracefully.
#[cfg(test)]
pub fn has_asset(name: &str) -> bool {
    ASSETS.iter().any(|(asset, _)| *asset == format!("icons/{name}.svg"))
}

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
    let name = interface_icon_name(name);
    svg()
        .path(format!("icons/{name}.svg"))
        .size(px(size))
        .flex_shrink_0()
        .text_color(theme::color(color))
}

// Colored file artwork becomes a solid silhouette when rendered as an alpha mask.
fn interface_icon_name(name: &str) -> &str {
    match name {
        "file" => "file-outline",
        "image" => "image-outline",
        "music" => "music-outline",
        "video" => "video-outline",
        _ => name,
    }
}

/// File artwork uses the image renderer to preserve SVG colors instead of a tinted mask.
pub fn file_icon(name: &str, size: f32) -> AnyElement {
    if name == "folder" {
        return icon(name, size, theme::ACCENT_BLUE).into_any_element();
    }
    img(format!("icons/{name}.svg"))
        .size(px(size))
        .flex_shrink_0()
        .into_any_element()
}

#[cfg(test)]
#[path = "../../tests/ui/icons.rs"]
mod tests;
