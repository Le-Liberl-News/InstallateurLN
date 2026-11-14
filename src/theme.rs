//! Each project's visual identity, from assets/themes.json: the structure of
//! the window never changes, only these values (colours, background motif,
//! transition style, pixel-art rendering).

use eframe::egui::Color32;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Motif {
    Orbits,
    Rays,
    Grid,
    Scanlines,
    Dots,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Transition {
    /// One diagonal blade sweeping across.
    Slash,
    /// Several diagonal blades, one after the other.
    Shutter,
    /// Horizontal bars, like a CRT redrawing.
    Scan,
}

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub bg: Color32,
    pub accent: Color32,
    pub accent2: Color32,
    pub motif: Motif,
    pub transition: Transition,
    /// Pictures enlarged without smoothing (pixel art).
    pub pixel: bool,
    /// Text, and secondary text.
    pub ink: Color32,
    pub muted: Color32,
    /// A light ground (dark text) or a dark one (CRT, night).
    pub light: bool,
}

/// Text on dark grounds: warm cream, and its dimmed form.
pub const TEXT: Color32 = Color32::from_rgb(0xf4, 0xea, 0xd8);
pub const DIM: Color32 = Color32::from_rgb(0xa8, 0x9c, 0x8a);
const INK: Color32 = Color32::from_rgb(0x1d, 0x22, 0x38);
const MUTED: Color32 = Color32::from_rgb(0x62, 0x6a, 0x80);

fn luminance(colour: Color32) -> f32 {
    (0.299 * colour.r() as f32 + 0.587 * colour.g() as f32 + 0.114 * colour.b() as f32) / 255.0
}

fn colour(value: Option<&Value>, fallback: Color32) -> Color32 {
    value
        .and_then(Value::as_str)
        .and_then(|text| u32::from_str_radix(text.trim_start_matches('#'), 16).ok())
        .map(|rgb| Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8))
        .unwrap_or(fallback)
}

impl Theme {
    /// Panels laid on the ground (the home's band, dialogs, the library's veil).
    pub fn surface(&self) -> Color32 {
        if self.light {
            crate::motion::mix(self.bg, self.accent, 0.08)
        } else {
            crate::motion::mix(self.bg, Color32::BLACK, 0.45)
        }
    }

    /// A shade of the ground for pictures' backs and shadows.
    pub fn shade(&self, amount: f32) -> Color32 {
        crate::motion::mix(self.bg, if self.light { self.ink } else { Color32::BLACK }, amount)
    }

    fn read(value: &Value, base: &Theme) -> Theme {
        let bg = colour(value.get("bg"), base.bg);
        let light = luminance(bg) > 0.5;
        Theme {
            light,
            ink: colour(value.get("ink"), if light { INK } else { TEXT }),
            muted: colour(value.get("muted"), if light { MUTED } else { DIM }),
            bg,
            accent: colour(value.get("accent"), base.accent),
            accent2: colour(value.get("accent2"), base.accent2),
            motif: match value.get("motif").and_then(Value::as_str) {
                Some("orbits") => Motif::Orbits,
                Some("rays") => Motif::Rays,
                Some("grid") => Motif::Grid,
                Some("scanlines") => Motif::Scanlines,
                Some("dots") => Motif::Dots,
                _ => base.motif,
            },
            transition: match value.get("transition").and_then(Value::as_str) {
                Some("slash") => Transition::Slash,
                Some("shutter") => Transition::Shutter,
                Some("scan") => Transition::Scan,
                _ => base.transition,
            },
            pixel: value.get("pixel").and_then(Value::as_bool).unwrap_or(false),
        }
    }
}

const FALLBACK: Theme = Theme {
    bg: Color32::from_rgb(0x17, 0x11, 0x0c),
    accent: Color32::from_rgb(0xe3, 0xa6, 0x4a),
    accent2: Color32::from_rgb(0x8c, 0x3b, 0x1f),
    motif: Motif::Rays,
    transition: Transition::Slash,
    pixel: false,
    ink: TEXT,
    muted: DIM,
    light: false,
};

pub struct Themes {
    default: Theme,
    by_project: HashMap<String, Theme>,
}

impl Themes {
    pub fn load() -> Themes {
        let json: Value = serde_json::from_str(include_str!("../assets/themes.json")).unwrap_or_default();
        let default = json.get("default").map(|value| Theme::read(value, &FALLBACK)).unwrap_or(FALLBACK);
        let by_project = json
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(key, _)| !key.starts_with('_') && *key != "default")
            .map(|(key, value)| (key.clone(), Theme::read(value, &default)))
            .collect();
        Themes { default, by_project }
    }

    pub fn of(&self, project: &str) -> Theme {
        self.by_project.get(project).copied().unwrap_or(self.default)
    }
}
