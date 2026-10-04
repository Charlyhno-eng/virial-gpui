//! Shared palette and dimensions for the initial dark theme.
use gpui::{Hsla, rgb};

pub const BACKGROUND: u32 = 0x171c24;
pub const SIDEBAR: u32 = 0x1b212b;
pub const SURFACE: u32 = 0x202733;
pub const HOVER: u32 = 0x2b3543;
pub const BORDER: u32 = 0x303a48;
pub const TEXT: u32 = 0xe9eef5;
pub const MUTED: u32 = 0x98a6b8;
pub const ACCENT: u32 = 0x79c7fa;
pub const SELECTED: u32 = 0x25445c;
pub const ERROR: u32 = 0xf2a6ad;
pub const ERROR_BG: u32 = 0x392832;
pub const ROW_HEIGHT: f32 = 46.;
pub const SIDEBAR_WIDTH: f32 = 208.;

pub fn color(value: u32) -> Hsla {
    rgb(value).into()
}
