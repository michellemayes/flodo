//! One accent colour derives an entire coherent palette.
//!
//! The trick that keeps this from looking like "a coloured button on grey" is
//! tinting the *neutrals* toward the accent's hue at very low saturation, so
//! the whole panel feels like it belongs to one colour family.

use eframe::egui::{self, Color32};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Accent {
    #[default]
    Pink,
    Purple,
    Blue,
    Teal,
    Green,
    Amber,
    Red,
    Graphite,
}

impl Accent {
    pub const ALL: [Accent; 8] = [
        Accent::Pink,
        Accent::Purple,
        Accent::Blue,
        Accent::Teal,
        Accent::Green,
        Accent::Amber,
        Accent::Red,
        Accent::Graphite,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Accent::Pink => "Pink",
            Accent::Purple => "Purple",
            Accent::Blue => "Blue",
            Accent::Teal => "Teal",
            Accent::Green => "Green",
            Accent::Amber => "Amber",
            Accent::Red => "Red",
            Accent::Graphite => "Graphite",
        }
    }

    /// Hue in degrees, used to tint the neutrals.
    fn hue_deg(self) -> f32 {
        match self {
            Accent::Pink => 336.0,
            Accent::Purple => 255.0,
            Accent::Blue => 208.0,
            Accent::Teal => 162.0,
            Accent::Green => 131.0,
            Accent::Amber => 36.0,
            Accent::Red => 0.0,
            Accent::Graphite => 210.0,
        }
    }

    /// Graphite is meant to read as genuinely neutral, so its tint is damped.
    fn tint_scale(self) -> f32 {
        if matches!(self, Accent::Graphite) {
            0.25
        } else {
            1.0
        }
    }

    /// Open Color shade 7 (light mode) and shade 3 (dark mode). Using a
    /// hue-balanced, contrast-tested palette means we get a coherent set for
    /// free instead of hand-tuning eight hues.
    fn accent_color(self, dark: bool) -> Color32 {
        let (light, dk) = match self {
            Accent::Pink => (0xd6336c, 0xf783ac),
            Accent::Purple => (0x7048e8, 0xb197fc),
            Accent::Blue => (0x1c7ed6, 0x74c0fc),
            Accent::Teal => (0x0ca678, 0x63e6be),
            Accent::Green => (0x37b24d, 0x8ce99a),
            Accent::Amber => (0xe8590c, 0xffd43b),
            Accent::Red => (0xe03131, 0xff8787),
            Accent::Graphite => (0x495057, 0xced4da),
        };
        rgb(if dark { dk } else { light })
    }
}

fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// Every colour the app draws with.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub surface: Color32,
    pub surface_hover: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub on_accent: Color32,
    pub dark: bool,
}

impl Palette {
    pub fn new(accent: Accent, dark: bool) -> Self {
        let h = accent.hue_deg() / 360.0;
        let k = accent.tint_scale();
        // HsvaGamma, not Hsva: egui's plain `Hsva` value is in *linear* space,
        // so a value of 0.45 renders as a mid-light grey rather than the
        // perceptual mid-grey these numbers are chosen for.
        let tint = |s: f32, v: f32| -> Color32 {
            egui::ecolor::HsvaGamma {
                h,
                s: s * k,
                v,
                a: 1.0,
            }
            .into()
        };

        let (bg, surface, surface_hover, border, text, muted) = if dark {
            (
                tint(0.10, 0.075),
                tint(0.10, 0.120),
                tint(0.10, 0.155),
                tint(0.10, 0.220),
                tint(0.04, 0.930),
                tint(0.05, 0.550),
            )
        } else {
            (
                tint(0.03, 1.000),
                tint(0.06, 0.965),
                tint(0.09, 0.940),
                tint(0.10, 0.880),
                tint(0.20, 0.130),
                tint(0.08, 0.450),
            )
        };

        let accent_color = accent.accent_color(dark);
        Self {
            bg,
            surface,
            surface_hover,
            border,
            text,
            muted,
            accent: accent_color,
            // The check glyph sits on the accent disc, so it must contrast
            // with the accent itself rather than with the background. Measure
            // both candidates instead of guessing a luminance threshold — the
            // light accents are bright enough that white would fail.
            on_accent: {
                let ink = rgb(0x11_11_11);
                if contrast_ratio(ink, accent_color) >= contrast_ratio(Color32::WHITE, accent_color)
                {
                    ink
                } else {
                    Color32::WHITE
                }
            },
            dark,
        }
    }

    pub fn selection(&self) -> Color32 {
        self.accent.gamma_multiply(0.25)
    }

    /// Used for the focus outline egui draws around the active text field.
    pub fn focus_ring(&self) -> Color32 {
        self.accent.gamma_multiply(0.45)
    }

    /// Applies the palette to egui's own widget styling so built-in widgets
    /// (text edits, sliders, scrollbars) match without per-widget overrides.
    pub fn apply(&self, style: &mut egui::Style) {
        let v = &mut style.visuals;
        v.dark_mode = self.dark;
        v.override_text_color = Some(self.text);
        v.panel_fill = self.bg;
        v.window_fill = self.bg;
        v.extreme_bg_color = self.surface;
        v.faint_bg_color = self.surface;
        v.selection.bg_fill = self.selection();
        v.selection.stroke = egui::Stroke::new(1.0, self.accent);
        v.widgets.active.fg_stroke = egui::Stroke::new(1.0, self.text);
        v.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, self.text);
        v.hyperlink_color = self.accent;
        v.window_stroke = egui::Stroke::new(1.0, self.border);
        // The ring egui draws around whichever widget has keyboard focus.
        v.widgets.active.bg_stroke = egui::Stroke::new(1.0, self.focus_ring());

        for w in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.corner_radius = 6.into();
            w.bg_stroke = egui::Stroke::new(1.0, self.border);
            w.fg_stroke = egui::Stroke::new(1.0, self.text);
        }
        v.widgets.noninteractive.bg_fill = self.surface;
        v.widgets.noninteractive.weak_bg_fill = self.surface;
        v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, self.muted);
        v.widgets.inactive.bg_fill = self.surface;
        v.widgets.inactive.weak_bg_fill = self.surface;
        v.widgets.hovered.bg_fill = self.surface_hover;
        v.widgets.hovered.weak_bg_fill = self.surface_hover;
        v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, self.accent);
        v.widgets.active.bg_fill = self.surface_hover;
        v.widgets.active.weak_bg_fill = self.surface_hover;
        v.widgets.active.bg_stroke = egui::Stroke::new(1.0, self.accent);

        style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, self.border);
    }
}

/// WCAG 2.1 relative luminance.
pub fn relative_luminance(c: Color32) -> f32 {
    fn channel(v: u8) -> f32 {
        let s = v as f32 / 255.0;
        if s <= 0.03928 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
}

/// WCAG 2.1 contrast ratio, 1.0 (identical) to 21.0 (black on white).
pub fn contrast_ratio(a: Color32, b: Color32) -> f32 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one automatable proxy for "beautiful": if a palette lands here with
    /// unreadable body text, this fails before anybody has to look at it.
    #[test]
    fn body_text_meets_wcag_aa_on_every_palette() {
        for accent in Accent::ALL {
            for dark in [true, false] {
                let p = Palette::new(accent, dark);
                let r = contrast_ratio(p.text, p.bg);
                assert!(
                    r >= 4.5,
                    "{:?} {} text:bg contrast {r:.2} < 4.5",
                    accent,
                    if dark { "dark" } else { "light" }
                );
            }
        }
    }

    #[test]
    fn muted_text_stays_legible_on_every_palette() {
        for accent in Accent::ALL {
            for dark in [true, false] {
                let p = Palette::new(accent, dark);
                let r = contrast_ratio(p.muted, p.bg);
                assert!(
                    r >= 3.0,
                    "{:?} {} muted:bg contrast {r:.2} < 3.0",
                    accent,
                    if dark { "dark" } else { "light" }
                );
            }
        }
    }

    #[test]
    fn check_glyph_contrasts_with_its_accent_disc() {
        for accent in Accent::ALL {
            for dark in [true, false] {
                let p = Palette::new(accent, dark);
                let r = contrast_ratio(p.on_accent, p.accent);
                assert!(
                    r >= 3.0,
                    "{:?} dark={dark} on_accent contrast {r:.2}",
                    accent
                );
            }
        }
    }

    #[test]
    fn surfaces_are_distinguishable_from_the_background() {
        for accent in Accent::ALL {
            for dark in [true, false] {
                let p = Palette::new(accent, dark);
                assert_ne!(p.surface, p.bg, "{accent:?} dark={dark}");
                assert_ne!(p.border, p.surface, "{accent:?} dark={dark}");
            }
        }
    }

    #[test]
    fn palette_is_deterministic() {
        assert_eq!(
            Palette::new(Accent::Teal, true).bg,
            Palette::new(Accent::Teal, true).bg
        );
    }

    #[test]
    fn graphite_is_less_saturated_than_a_coloured_accent() {
        let spread = |c: Color32| {
            let (r, g, b) = (c.r() as i32, c.g() as i32, c.b() as i32);
            r.max(g).max(b) - r.min(g).min(b)
        };
        let graphite = Palette::new(Accent::Graphite, true);
        let pink = Palette::new(Accent::Pink, true);
        assert!(spread(graphite.bg) < spread(pink.bg));
    }
}
