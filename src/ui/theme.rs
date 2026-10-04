//! Compact, subdued dark theme inspired by the Virial logo.
use gpui::{Hsla, rgb};

pub const BACKGROUND: u32 = 0x0e0f16;
pub const SIDEBAR: u32 = 0x14181e;
pub const SURFACE: u32 = 0x1a1f26;
pub const HOVER: u32 = 0x292f38;
pub const BORDER: u32 = 0x303740;
pub const TEXT: u32 = 0xe4e7eb;
pub const MUTED: u32 = 0xa3aab3;
pub const ACCENT: u32 = 0xa6a8c4;
pub const ACCENT_BLUE: u32 = 0x9eafbf;
pub const SELECTED: u32 = 0x2a303a;
pub const ERROR: u32 = 0xf2a6ad;
pub const ERROR_BG: u32 = 0x392832;
pub const ROW_HEIGHT: f32 = 34.;
pub const SIDEBAR_WIDTH: f32 = 176.;

pub fn color(value: u32) -> Hsla {
    rgb(value).into()
}

pub fn translucent(value: u32, opacity: f32) -> Hsla {
    let mut color = color(value);
    color.a = opacity;
    color
}
