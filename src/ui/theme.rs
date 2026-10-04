//! Compact, subdued dark theme inspired by the Virial logo.
use gpui::{Hsla, rgb};

pub const BACKGROUND: u32 = 0x0f0e17;
pub const SIDEBAR: u32 = 0x16151f;
pub const SURFACE: u32 = 0x1c1b28;
pub const HOVER: u32 = 0x2b2a39;
pub const BORDER: u32 = 0x323140;
pub const TEXT: u32 = 0xe4e7eb;
pub const MUTED: u32 = 0xa3aab3;
pub const ACCENT: u32 = 0xaaa6c8;
pub const ACCENT_BLUE: u32 = 0xa3a7c4;
pub const SELECTED: u32 = 0x2c2b3d;
pub const ERROR: u32 = 0xf2a6ad;
pub const ERROR_BG: u32 = 0x392832;
pub const ROW_HEIGHT: f32 = 34. * 0.78;
pub const SIDEBAR_WIDTH: f32 = 176.;

pub fn color(value: u32) -> Hsla {
    rgb(value).into()
}

pub fn translucent(value: u32, opacity: f32) -> Hsla {
    let mut color = color(value);
    color.a = opacity;
    color
}
