//! System font enumeration and loading.
//!
//! epaint panics outright on font bytes it cannot parse:
//!
//! ```text
//! .unwrap_or_else(|err| panic!("Error parsing {name:?} TTF/OTF font file: {err}"))
//! ```
//!
//! The only fallible step inside is `skrifa::FontRef::from_index`, so every
//! byte slice is validated with skrifa *before* it reaches `set_fonts`. A font
//! we can't parse is simply never offered.

use crate::markdown::{FAMILY_BOLD, FAMILY_BOLD_ITALIC, FAMILY_ITALIC, FAMILY_MONO, FAMILY_UI};
use crate::settings::FontChoice;
use eframe::egui::{self, FontFamily};
use skrifa::MetadataProvider;
use std::sync::Arc;

/// One selectable font family.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FamilyInfo {
    pub name: String,
    pub path: String,
    pub index: u32,
    pub monospaced: bool,
}

/// Returns `true` only if epaint will be able to parse these bytes.
///
/// Deliberately uses the *same* skrifa version epaint does (pinned in
/// Cargo.toml), so this can't accept something epaint would then reject.
pub fn is_loadable(bytes: &[u8], index: u32) -> bool {
    let Ok(font) = skrifa::FontRef::from_index(bytes, index) else {
        return false;
    };
    // A face with no outlines (bitmap-only, e.g. colour emoji) parses fine but
    // renders as blank boxes, so treat it as unusable too.
    font.outline_glyphs().format().is_some()
}

/// Scans the system font database. Slow enough (~100-300ms on macOS) that this
/// must not run on the startup path — call it lazily from a background thread.
pub fn enumerate() -> Vec<FamilyInfo> {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();

    let mut out: Vec<FamilyInfo> = Vec::new();
    for face in db.faces() {
        // Only regular upright faces become selectable families; bold and
        // italic are resolved from the family later.
        if face.style != fontdb::Style::Normal || face.weight != fontdb::Weight::NORMAL {
            continue;
        }
        let fontdb::Source::File(path) = &face.source else {
            continue;
        };
        let Some((name, _)) = face.families.first() else {
            continue;
        };
        // Apple's internal families are noise in a picker.
        if name.starts_with('.') {
            continue;
        }
        if out.iter().any(|f| f.name == *name) {
            continue;
        }
        // Never offer a font we couldn't actually load. Validating through
        // fontdb's mmap avoids a second read of every file on disk. This is
        // what keeps bitmap-only faces (colour emoji) out of the picker.
        if db.with_face_data(face.id, is_loadable) != Some(true) {
            continue;
        }
        out.push(FamilyInfo {
            name: name.clone(),
            path: path.to_string_lossy().to_string(),
            index: face.index,
            monospaced: face.monospaced,
        });
    }

    out.sort();
    out
}

fn read(path: &str) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

/// Loads a chosen font's bytes, returning `None` if anything at all goes wrong
/// — a missing file, an uninstalled font, or unparsable data.
fn load_validated(choice: &FontChoice) -> Option<(Arc<Vec<u8>>, u32)> {
    if choice.is_default() {
        return None;
    }
    let bytes = read(&choice.path)?;
    if !is_loadable(&bytes, choice.index) {
        eprintln!(
            "flodo: font {:?} could not be parsed; falling back to the built-in font",
            choice.family
        );
        return None;
    }
    Some((Arc::new(bytes), choice.index))
}

/// Finds the face in `family` matching a weight/style, for real bold and italic
/// (egui has no synthetic emphasis).
fn find_face(
    db: &fontdb::Database,
    family: &str,
    weight: fontdb::Weight,
    style: fontdb::Style,
) -> Option<(Vec<u8>, u32)> {
    // Oblique counts as italic: plenty of families (DejaVu Sans, Helvetica)
    // ship a slanted face labelled Oblique and no Italic at all.
    let style_matches = |f: &fontdb::FaceInfo| match style {
        fontdb::Style::Italic => matches!(f.style, fontdb::Style::Italic | fontdb::Style::Oblique),
        s => f.style == s,
    };
    let face = db.faces().find(|f| {
        f.weight == weight
            && style_matches(f)
            && f.families.iter().any(|(n, _)| n == family)
            && matches!(f.source, fontdb::Source::File(_))
    })?;
    let fontdb::Source::File(path) = &face.source else {
        return None;
    };
    let bytes = read(&path.to_string_lossy())?;
    is_loadable(&bytes, face.index).then_some((bytes, face.index))
}

/// True when this face exposes a variable axis (e.g. `wght`) we can instance.
fn has_axis(bytes: &[u8], index: u32, tag: &[u8; 4]) -> bool {
    let Ok(font) = skrifa::FontRef::from_index(bytes, index) else {
        return false;
    };
    font.axes().iter().any(|a| &a.tag().to_be_bytes() == tag)
}

fn insert(
    defs: &mut egui::FontDefinitions,
    key: &str,
    bytes: Arc<Vec<u8>>,
    index: u32,
    tweak: egui::FontTweak,
) {
    let data = egui::FontData {
        font: std::borrow::Cow::Owned(bytes.as_ref().clone()),
        index,
        tweak,
    };
    defs.font_data.insert(key.to_owned(), Arc::new(data));
    defs.families
        .insert(FontFamily::Name(key.into()), vec![key.to_owned()]);
}

/// Points a named family at egui's bundled default, so every family we
/// reference always resolves to *something*.
fn alias_to_default(defs: &mut egui::FontDefinitions, key: &str, mono: bool) {
    let fallback = if mono {
        FontFamily::Monospace
    } else {
        FontFamily::Proportional
    };
    let stack = defs.families.get(&fallback).cloned().unwrap_or_default();
    defs.families.insert(FontFamily::Name(key.into()), stack);
}

/// Builds the five named families the UI draws with. Never panics and never
/// leaves a family unresolved.
pub fn build(ui_choice: &FontChoice, mono_choice: &FontChoice) -> egui::FontDefinitions {
    let mut defs = egui::FontDefinitions::default();

    // Proportional: regular, plus real bold/italic where they exist.
    match load_validated(ui_choice) {
        None => {
            for key in [FAMILY_UI, FAMILY_BOLD, FAMILY_ITALIC, FAMILY_BOLD_ITALIC] {
                alias_to_default(&mut defs, key, false);
            }
        }
        Some((bytes, index)) => {
            insert(
                &mut defs,
                FAMILY_UI,
                bytes.clone(),
                index,
                egui::FontTweak::default(),
            );

            let mut db = fontdb::Database::new();
            db.load_system_fonts();

            let variants = [
                (FAMILY_BOLD, fontdb::Weight::BOLD, fontdb::Style::Normal),
                (FAMILY_ITALIC, fontdb::Weight::NORMAL, fontdb::Style::Italic),
                (
                    FAMILY_BOLD_ITALIC,
                    fontdb::Weight::BOLD,
                    fontdb::Style::Italic,
                ),
            ];
            for (key, weight, style) in variants {
                if let Some((b, i)) = find_face(&db, &ui_choice.family, weight, style) {
                    insert(&mut defs, key, Arc::new(b), i, egui::FontTweak::default());
                    continue;
                }
                // No static face — try instancing a variable axis on the
                // regular file (this is what covers SF Pro and Inter).
                let wants_bold = weight == fontdb::Weight::BOLD;
                let wants_italic = style == fontdb::Style::Italic;
                let can_bold = !wants_bold || has_axis(&bytes, index, b"wght");
                let has_ital = has_axis(&bytes, index, b"ital");
                let can_italic = !wants_italic || has_ital || has_axis(&bytes, index, b"slnt");

                if can_bold && can_italic {
                    let mut coords = egui::epaint::text::VariationCoords::default();
                    if wants_bold {
                        coords.push(b"wght", 700.0);
                    }
                    if wants_italic {
                        if has_ital {
                            coords.push(b"ital", 1.0);
                        } else {
                            coords.push(b"slnt", -10.0);
                        }
                    }
                    insert(
                        &mut defs,
                        key,
                        bytes.clone(),
                        index,
                        egui::FontTweak {
                            coords,
                            ..Default::default()
                        },
                    );
                } else {
                    // Honest fallback: emphasis renders as regular rather than
                    // being faked with a skew.
                    insert(
                        &mut defs,
                        key,
                        bytes.clone(),
                        index,
                        egui::FontTweak::default(),
                    );
                }
            }
        }
    }

    match load_validated(mono_choice) {
        Some((bytes, index)) => insert(
            &mut defs,
            FAMILY_MONO,
            bytes,
            index,
            egui::FontTweak::default(),
        ),
        None => alias_to_default(&mut defs, FAMILY_MONO, true),
    }

    // Every named family falls back to the bundled fonts for glyphs it lacks
    // (emoji, CJK), instead of drawing tofu.
    let proportional = defs
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    for key in [
        FAMILY_UI,
        FAMILY_BOLD,
        FAMILY_ITALIC,
        FAMILY_BOLD_ITALIC,
        FAMILY_MONO,
    ] {
        if let Some(stack) = defs.families.get_mut(&FontFamily::Name(key.into())) {
            for f in &proportional {
                if !stack.contains(f) {
                    stack.push(f.clone());
                }
            }
        }
    }

    defs
}

fn first_of(available: &[FamilyInfo], preferred: &[&str]) -> Option<FamilyInfo> {
    preferred
        .iter()
        .find_map(|want| available.iter().find(|f| f.name == *want).cloned())
}

/// Picks a sensible monospace default per platform, used on first run.
pub fn default_mono(available: &[FamilyInfo]) -> Option<FamilyInfo> {
    first_of(
        available,
        &[
            "SF Mono",
            "Menlo",
            "Monaco",
            "JetBrains Mono",
            "Cascadia Mono",
            "Consolas",
            "DejaVu Sans Mono",
            "Liberation Mono",
        ],
    )
    .or_else(|| available.iter().find(|f| f.monospaced).cloned())
}

/// Picks a sensible proportional default.
///
/// This matters beyond taste: egui ships only Ubuntu-Light, which has no bold
/// or italic face, so `**bold**` and `*italic*` cannot render at all on the
/// built-in font. Defaulting to a real system family is what makes markdown
/// emphasis work out of the box.
pub fn default_proportional(available: &[FamilyInfo]) -> Option<FamilyInfo> {
    first_of(
        available,
        &[
            "SF Pro Text",
            "SF Pro",
            "Helvetica Neue",
            "Segoe UI Variable Text",
            "Segoe UI",
            "Inter",
            "DejaVu Sans",
            "Liberation Sans",
            "Noto Sans",
        ],
    )
    .or_else(|| available.iter().find(|f| !f.monospaced).cloned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bytes epaint would accept. Uses a font egui itself bundles, so the test
    /// needs nothing from the host system.
    fn good_font_bytes() -> Vec<u8> {
        let defs = egui::FontDefinitions::default();
        let (_, data) = defs
            .font_data
            .iter()
            .next()
            .expect("egui ships default fonts");
        data.font.to_vec()
    }

    /// This is the guard that keeps a user's font choice from panicking the
    /// app inside epaint. If it ever stops rejecting junk, Flodo crashes.
    #[test]
    fn validation_rejects_anything_epaint_would_panic_on() {
        assert!(!is_loadable(b"", 0));
        assert!(!is_loadable(b"not a font", 0));
        assert!(!is_loadable(&[0u8; 512], 0));
        assert!(!is_loadable(&vec![0xff; 4096], 0));

        let good = good_font_bytes();
        assert!(!is_loadable(&good[..good.len() / 4], 0), "truncated");
        // A collection index that doesn't exist must be refused, not trusted.
        assert!(!is_loadable(&good, 99));
    }

    #[test]
    fn validation_accepts_a_real_font() {
        assert!(is_loadable(&good_font_bytes(), 0));
    }

    #[test]
    fn a_missing_font_file_falls_back_instead_of_failing() {
        let bogus = FontChoice {
            family: "Nonexistent".into(),
            path: "/definitely/not/here.ttf".into(),
            index: 0,
        };
        assert!(load_validated(&bogus).is_none());

        // ...and building with it still produces every family we draw with.
        let defs = build(&bogus, &bogus);
        for key in [
            FAMILY_UI,
            FAMILY_BOLD,
            FAMILY_ITALIC,
            FAMILY_BOLD_ITALIC,
            FAMILY_MONO,
        ] {
            let fam = defs.families.get(&FontFamily::Name(key.into()));
            assert!(fam.is_some_and(|s| !s.is_empty()), "{key} unresolved");
        }
    }

    #[test]
    fn the_default_choice_builds_a_complete_definition_set() {
        let defs = build(&FontChoice::default(), &FontChoice::default());
        for key in [FAMILY_UI, FAMILY_BOLD, FAMILY_MONO] {
            assert!(defs.families.contains_key(&FontFamily::Name(key.into())));
        }
    }

    #[test]
    fn a_font_that_does_not_parse_is_never_offered() {
        let dir = std::env::temp_dir().join(format!(
            "flodo-badfont-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("junk.ttf");
        std::fs::write(&path, b"absolutely not a font").unwrap();

        let choice = FontChoice {
            family: "Junk".into(),
            path: path.to_string_lossy().to_string(),
            index: 0,
        };
        assert!(load_validated(&choice).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn enumeration_only_returns_fonts_we_can_actually_load() {
        // The build container has fonts installed; if it ever doesn't, this
        // still passes vacuously rather than failing spuriously.
        for f in enumerate().iter().take(40) {
            assert!(!f.name.starts_with('.'), "internal family leaked: {f:?}");
            if let Some(bytes) = read(&f.path) {
                assert!(
                    is_loadable(&bytes, f.index),
                    "offered an unloadable font: {f:?}"
                );
            }
        }
    }
}
