//! Window geometry and adjustable preview width, saved across launches.
use gpui::{Bounds, Pixels, Window, WindowBounds, point, px, size};
use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
};

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Layout {
    pub window: Option<Bounds<Pixels>>,
    pub maximized: bool,
    pub fullscreen: bool,
    pub preview_width: Option<f32>,
}

fn path() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/"))
                .join(".config")
        })
        .join("virial/layout")
}

impl Layout {
    pub(crate) fn load() -> Self {
        fs::read_to_string(path())
            .ok()
            .and_then(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    fn parse(text: &str) -> Option<Self> {
        let mut layout = Self::default();
        for line in text.lines() {
            let (key, value) = line.split_once('=')?;
            match key {
                "window" => {
                    let values = value
                        .split(',')
                        .map(str::parse::<f32>)
                        .collect::<Result<Vec<_>, _>>()
                        .ok()?;
                    let [x, y, width, height] = values.as_slice() else {
                        return None;
                    };
                    if !values
                        .iter()
                        .all(|value| value.is_finite() && value.abs() <= 32768.)
                        || *width < 720.
                        || *height < 420.
                    {
                        return None;
                    }
                    layout.window = Some(Bounds::new(
                        point(px(*x), px(*y)),
                        size(px(*width), px(*height)),
                    ));
                }
                "maximized" => layout.maximized = value.parse().ok()?,
                "fullscreen" => layout.fullscreen = value.parse().ok()?,
                "preview_width" => {
                    let width: f32 = value.parse().ok()?;
                    if !width.is_finite() || !(160. ..=32768.).contains(&width) {
                        return None;
                    }
                    layout.preview_width = Some(width);
                }
                _ => return None,
            }
        }
        Some(layout)
    }

    fn encode(&self) -> String {
        let mut text = format!(
            "maximized={}\nfullscreen={}\n",
            self.maximized, self.fullscreen
        );
        if let Some(bounds) = self.window {
            text.push_str(&format!(
                "window={},{},{},{}\n",
                f32::from(bounds.origin.x),
                f32::from(bounds.origin.y),
                f32::from(bounds.size.width),
                f32::from(bounds.size.height)
            ));
        }
        if let Some(width) = self.preview_width {
            text.push_str(&format!("preview_width={width}\n"));
        }
        text
    }

    pub(crate) fn save(&self) {
        if let Err(error) = self.write(&path()) {
            eprintln!("Cannot save Virial's layout: {error}");
        }
    }

    fn write(&self, path: &std::path::Path) -> io::Result<()> {
        let directory = path
            .parent()
            .ok_or_else(|| io::Error::other("Missing layout directory"))?;
        fs::create_dir_all(directory)?;
        let mut file = tempfile::NamedTempFile::new_in(directory)?;
        file.write_all(self.encode().as_bytes())?;
        file.persist(path).map_err(|error| error.error)?;
        Ok(())
    }

    pub(crate) fn capture(&mut self, window: &Window) {
        self.maximized = window.is_maximized();
        self.fullscreen = window.is_fullscreen();
        if !self.maximized && !self.fullscreen {
            self.window = Some(window.bounds());
        }
    }

    pub(crate) fn bounds(&self, fallback: Bounds<Pixels>, cx: &gpui::App) -> WindowBounds {
        let bounds = self
            .window
            .and_then(|mut bounds| {
                let display = cx.displays().into_iter().find(|display| {
                    let visible = bounds.intersect(&display.bounds());
                    visible.size.width >= px(100.) && visible.size.height >= px(100.)
                })?;
                let screen = display.bounds();
                bounds.size = bounds.size.min(&screen.size);
                bounds.origin.x = bounds
                    .origin
                    .x
                    .max(screen.origin.x)
                    .min(screen.right() - bounds.size.width);
                bounds.origin.y = bounds
                    .origin
                    .y
                    .max(screen.origin.y)
                    .min(screen.bottom() - bounds.size.height);
                Some(bounds)
            })
            .unwrap_or(fallback);
        if self.fullscreen {
            WindowBounds::Fullscreen(bounds)
        } else if self.maximized {
            WindowBounds::Maximized(bounds)
        } else {
            WindowBounds::Windowed(bounds)
        }
    }
}

#[cfg(test)]
#[path = "../../tests/infrastructure/layout.rs"]
mod tests;
