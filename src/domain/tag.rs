//! The eight optional, cumulative color labels.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tag {
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
    Pink,
    Gray,
}

impl Tag {
    pub const ALL: [Self; 8] = [
        Self::Red,
        Self::Orange,
        Self::Yellow,
        Self::Green,
        Self::Blue,
        Self::Purple,
        Self::Pink,
        Self::Gray,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Red => "Red",
            Self::Orange => "Orange",
            Self::Yellow => "Yellow",
            Self::Green => "Green",
            Self::Blue => "Blue",
            Self::Purple => "Purple",
            Self::Pink => "Pink",
            Self::Gray => "Gray",
        }
    }

    pub fn color(self) -> u32 {
        match self {
            Self::Red => 0xf07178,
            Self::Orange => 0xf3a65a,
            Self::Yellow => 0xe8ce6a,
            Self::Green => 0x82c995,
            Self::Blue => 0x73a9ef,
            Self::Purple => 0xb394ed,
            Self::Pink => 0xe79bc9,
            Self::Gray => 0x9ca3b4,
        }
    }
}
