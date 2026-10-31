use std::path::PathBuf;

use eframe::egui::Color32;
use serde::{Deserialize, Serialize};

pub const DEFAULT_FONT_SIZE: f32 = 16.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    Classic,
    Amber,
    Green,
    Paper,
}

impl Theme {
    pub fn next(self) -> Self {
        match self {
            Theme::Classic => Theme::Amber,
            Theme::Amber => Theme::Green,
            Theme::Green => Theme::Paper,
            Theme::Paper => Theme::Classic,
        }
    }

    /// (foreground, background)
    pub fn colors(self) -> (Color32, Color32) {
        match self {
            Theme::Classic => (Color32::from_rgb(0xD0, 0xD0, 0xD0), Color32::from_rgb(0x10, 0x10, 0x10)),
            Theme::Amber => (Color32::from_rgb(0xFF, 0xB0, 0x00), Color32::from_rgb(0x14, 0x0C, 0x00)),
            Theme::Green => (Color32::from_rgb(0x33, 0xFF, 0x66), Color32::from_rgb(0x00, 0x12, 0x06)),
            Theme::Paper => (Color32::from_rgb(0x20, 0x20, 0x20), Color32::from_rgb(0xF4, 0xF1, 0xE8)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub last_story: Option<PathBuf>,
    pub font_size: f32,
    pub theme: Theme,
}

impl Default for Settings {
    fn default() -> Self {
        Self { last_story: None, font_size: DEFAULT_FONT_SIZE, theme: Theme::Classic }
    }
}
