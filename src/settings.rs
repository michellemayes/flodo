//! User preferences. Seven knobs, no more.

use crate::theme::Accent;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const SETTINGS_VERSION: u32 = 1;

pub const FONT_SIZE_RANGE: (f32, f32) = (11.0, 20.0);
pub const SPACING_RANGE: (f32, f32) = (0.0, 20.0);
pub const OPACITY_RANGE: (f32, f32) = (0.70, 1.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    #[default]
    Auto,
    Light,
    Dark,
}

impl Appearance {
    pub const ALL: [Appearance; 3] = [Appearance::Auto, Appearance::Light, Appearance::Dark];

    pub fn label(self) -> &'static str {
        match self {
            Appearance::Auto => "Auto",
            Appearance::Light => "Light",
            Appearance::Dark => "Dark",
        }
    }
}

/// A chosen font, remembered by path + face index so startup never has to
/// enumerate the system font database.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct FontChoice {
    /// Empty means "use the built-in font".
    #[serde(default)]
    pub family: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub index: u32,
}

impl FontChoice {
    pub fn is_default(&self) -> bool {
        self.family.is_empty() || self.path.is_empty()
    }

    pub fn label(&self) -> &str {
        if self.is_default() {
            "Built-in"
        } else {
            &self.family
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowState {
    /// `None` until the window has been moved. Deliberately an `Option` and
    /// not a NaN sentinel: `serde_json` writes NaN as `null`, which then fails
    /// to deserialize back into `f32` and would quarantine the settings file
    /// on the very next launch.
    #[serde(default)]
    pub x: Option<f32>,
    #[serde(default)]
    pub y: Option<f32>,
    #[serde(default = "default_win_w")]
    pub w: f32,
    #[serde(default = "default_win_h")]
    pub h: f32,
}

fn default_win_w() -> f32 {
    340.0
}
fn default_win_h() -> f32 {
    480.0
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            x: None,
            y: None,
            w: default_win_w(),
            h: default_win_h(),
        }
    }
}

impl WindowState {
    pub fn position(&self) -> Option<(f32, f32)> {
        match (self.x, self.y) {
            (Some(x), Some(y)) if x.is_finite() && y.is_finite() => Some((x, y)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub accent: Accent,
    #[serde(default)]
    pub appearance: Appearance,
    #[serde(default)]
    pub font: FontChoice,
    #[serde(default)]
    pub mono_font: FontChoice,
    /// False until the user has picked a font at least once. Distinguishes
    /// "never chose one, go find a good default" from "deliberately chose the
    /// built-in font", which otherwise look identical and would make us
    /// override the choice on every launch.
    #[serde(default)]
    pub fonts_chosen: bool,
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    #[serde(default = "default_spacing")]
    pub spacing: f32,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    #[serde(default)]
    pub hide_completed: bool,
    #[serde(default = "default_true")]
    pub always_on_top: bool,
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    #[serde(default)]
    pub window: WindowState,
    #[serde(flatten, default, skip_serializing_if = "Map::is_empty")]
    pub extra: Map<String, Value>,
}

fn default_version() -> u32 {
    SETTINGS_VERSION
}
fn default_font_size() -> f32 {
    14.0
}
fn default_spacing() -> f32 {
    6.0
}
fn default_opacity() -> f32 {
    1.0
}
fn default_true() -> bool {
    true
}
fn default_hotkey() -> String {
    "Alt+Space".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            accent: Accent::default(),
            appearance: Appearance::default(),
            font: FontChoice::default(),
            mono_font: FontChoice::default(),
            fonts_chosen: false,
            font_size: default_font_size(),
            spacing: default_spacing(),
            opacity: default_opacity(),
            hide_completed: false,
            always_on_top: true,
            hotkey: default_hotkey(),
            window: WindowState::default(),
            extra: Map::new(),
        }
    }
}

impl Settings {
    /// Files are hand-editable, so anything numeric gets clamped on the way in
    /// rather than trusted.
    pub fn clamp(&mut self) {
        self.font_size = self.font_size.clamp(FONT_SIZE_RANGE.0, FONT_SIZE_RANGE.1);
        self.spacing = self.spacing.clamp(SPACING_RANGE.0, SPACING_RANGE.1);
        self.opacity = self.opacity.clamp(OPACITY_RANGE.0, OPACITY_RANGE.1);
        if !self.font_size.is_finite() {
            self.font_size = default_font_size();
        }
        if !self.spacing.is_finite() {
            self.spacing = default_spacing();
        }
        if !self.opacity.is_finite() {
            self.opacity = default_opacity();
        }
        self.window.w = if self.window.w.is_finite() {
            self.window.w.clamp(260.0, 4000.0)
        } else {
            default_win_w()
        };
        self.window.h = if self.window.h.is_finite() {
            self.window.h.clamp(180.0, 4000.0)
        } else {
            default_win_h()
        };
        // A non-finite coordinate can't be honoured, so forget it entirely.
        if self.window.position().is_none() {
            self.window.x = None;
            self.window.y = None;
        }
    }

    /// Resolves `Auto` against the OS-reported appearance.
    pub fn is_dark(&self, system_dark: bool) -> bool {
        match self.appearance {
            Appearance::Auto => system_dark,
            Appearance::Light => false,
            Appearance::Dark => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let s = Settings::default();
        assert_eq!(s.font_size, 14.0);
        assert_eq!(s.spacing, 6.0);
        assert_eq!(s.opacity, 1.0);
        assert!(s.always_on_top);
        assert!(!s.hide_completed);
        assert!(s.font.is_default());
    }

    #[test]
    fn an_empty_object_deserializes_to_defaults() {
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(s, Settings::default());
    }

    #[test]
    fn hand_edited_out_of_range_values_are_clamped() {
        let mut s: Settings = serde_json::from_str(
            r#"{"font_size": 400.0, "spacing": -50.0, "opacity": 9.0,
                "window": {"x": 0.0, "y": 0.0, "w": 1.0, "h": 99999.0}}"#,
        )
        .unwrap();
        s.clamp();
        assert_eq!(s.font_size, FONT_SIZE_RANGE.1);
        assert_eq!(s.spacing, SPACING_RANGE.0);
        assert_eq!(s.opacity, OPACITY_RANGE.1);
        assert_eq!(s.window.w, 260.0);
        assert_eq!(s.window.h, 4000.0);
    }

    #[test]
    fn non_finite_values_fall_back_to_defaults() {
        let mut s = Settings {
            font_size: f32::NAN,
            spacing: f32::INFINITY,
            opacity: f32::NAN,
            ..Default::default()
        };
        s.clamp();
        assert_eq!(s.font_size, 14.0);
        assert_eq!(s.spacing, SPACING_RANGE.1);
        assert_eq!(s.opacity, 1.0);
    }

    #[test]
    fn unknown_settings_keys_survive_a_round_trip() {
        let s: Settings =
            serde_json::from_str(r#"{"accent":"teal","somethingNew":{"deep":true}}"#).unwrap();
        assert_eq!(s.accent, Accent::Teal);
        let v: Value = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(v["somethingNew"]["deep"], true);
    }

    #[test]
    fn appearance_auto_follows_the_system() {
        let mut s = Settings::default();
        assert!(s.is_dark(true));
        assert!(!s.is_dark(false));
        s.appearance = Appearance::Dark;
        assert!(s.is_dark(false));
        s.appearance = Appearance::Light;
        assert!(!s.is_dark(true));
    }

    /// Regression: font adoption used to be gated on "no settings file yet",
    /// so a settings file written without a font never got a system default
    /// and markdown emphasis silently stopped rendering.
    #[test]
    fn a_settings_file_without_a_font_has_not_chosen_one() {
        let s: Settings = serde_json::from_str(r#"{"accent":"teal"}"#).unwrap();
        assert!(!s.fonts_chosen, "must still be looking for a default");
        assert!(s.font.is_default());
    }

    #[test]
    fn an_explicit_built_in_choice_is_distinguishable_from_never_choosing() {
        let never = Settings::default();
        let chose_built_in = Settings {
            fonts_chosen: true,
            ..Default::default()
        };
        // Both have an empty FontChoice...
        assert!(never.font.is_default() && chose_built_in.font.is_default());
        // ...but only one of them still wants a default picked for it.
        assert!(!never.fonts_chosen);
        assert!(chose_built_in.fonts_chosen);
    }

    #[test]
    fn window_position_is_absent_until_set() {
        assert_eq!(WindowState::default().position(), None);
        let w = WindowState {
            x: Some(10.0),
            y: Some(20.0),
            ..Default::default()
        };
        assert_eq!(w.position(), Some((10.0, 20.0)));
    }

    /// Regression: a NaN sentinel serialized to `null` and then failed to load,
    /// which quarantined the settings file on every launch after the first.
    #[test]
    fn settings_survive_a_serialize_then_deserialize_cycle() {
        let mut s = Settings::default();
        s.window.x = Some(120.0);
        s.window.y = Some(64.0);
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);

        // ...and the same holds for the never-positioned default.
        let d = Settings::default();
        let back: Settings = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
        assert_eq!(back, d);
    }
}
