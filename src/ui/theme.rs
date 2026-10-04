//! Compact dark theme with subtle violet accents.
use gpui::{Hsla, rgb};

pub const BACKGROUND: u32 = 0x100e18;
pub const SIDEBAR: u32 = 0x171520;
pub const SURFACE: u32 = 0x1d1b2a;
pub const HOVER: u32 = 0x2d2a3b;
pub const BORDER: u32 = 0x353143;
pub const TEXT: u32 = 0xe4e7eb;
pub const MUTED: u32 = 0xaaa5ba;
pub const ACCENT: u32 = 0xb3a8d9;
pub const ACCENT_BLUE: u32 = 0xaca4d4;
pub const SELECTED: u32 = 0x302b43;
pub const ERROR: u32 = 0xf2a6ad;
pub const ERROR_BG: u32 = 0x392832;
pub const ROW_HEIGHT: f32 = 34. * 0.78;
pub const RECENT_ROW_HEIGHT: f32 = 46.;
pub const SIDEBAR_WIDTH: f32 = 176.;

pub fn color(value: u32) -> Hsla {
    rgb(value).into()
}

pub fn translucent(value: u32, opacity: f32) -> Hsla {
    let mut color = color(value);
    color.a = opacity;
    color
}
