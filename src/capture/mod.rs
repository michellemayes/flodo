//! Quick capture: double-tap a modifier anywhere to summon Flodo, bringing
//! whatever text you had selected along with you.
//!
//! The summon hotkey in [`crate::hotkey`] registers through Carbon, which
//! needs no permission. This does not: a bare modifier can't be registered as
//! a shortcut, so hearing one means watching the whole keyboard, and reading
//! another app's selection means asking that app. Both are Accessibility, and
//! both are why quick capture is off until you turn it on.
//!
//! Everything that decides *what* a double-tap means lives in this file and is
//! unit tested. The platform backend only reports raw transitions and reads
//! the selection; it holds no policy of its own.

// That split has one cost: the detector and the splitter are driven by the
// backend, and off macOS there isn't one, so on those targets the only caller
// is the test module. Keeping them compiled and tested everywhere is the point
// — the logic is where the bugs would be, and it is the same logic on every
// platform — but it does mean telling the dead-code lint so. Only off macOS:
// unreachable code on the platform that does have a backend is a real finding.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use serde::{Deserialize, Serialize};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as backend;

#[cfg(not(target_os = "macos"))]
mod unsupported;
#[cfg(not(target_os = "macos"))]
use unsupported as backend;

/// A long selection becomes a to-do, not an archive. Past this the body is
/// trimmed — the point is to remember the thing, not to hold the document.
pub const BODY_MAX: usize = 2000;

/// Past this the first line stops being a title and starts being a paragraph,
/// so it gets cut short and the whole of it is kept in the body instead.
pub const TITLE_MAX: usize = 120;

/// The gap allowed between the two taps.
pub const WINDOW_MS_RANGE: (u64, u64) = (150, 900);

/// Held longer than this and it wasn't a tap. Without it, leaning on Shift
/// while you think, then releasing and pressing it again, reads as a
/// double-tap.
const HOLD_MAX: Duration = Duration::from_millis(500);

/// The modifier to listen for. Deliberately not every key: this only ever
/// watches one modifier, so nothing you type is inspected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TapKey {
    #[default]
    Shift,
    Control,
    Alt,
    Command,
}

impl TapKey {
    pub const ALL: [TapKey; 4] = [TapKey::Shift, TapKey::Control, TapKey::Alt, TapKey::Command];

    /// What the key is called on this platform, for the settings sheet.
    pub fn label(self) -> &'static str {
        match self {
            TapKey::Shift => "Shift",
            TapKey::Control => "Control",
            #[cfg(target_os = "macos")]
            TapKey::Alt => "Option",
            #[cfg(not(target_os = "macos"))]
            TapKey::Alt => "Alt",
            #[cfg(target_os = "macos")]
            TapKey::Command => "Command",
            #[cfg(not(target_os = "macos"))]
            TapKey::Command => "Super",
        }
    }
}

/// What the keyboard just did, as far as quick capture cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    /// The watched modifier went down.
    Down,
    /// The watched modifier came up.
    Up,
    /// Anything else at all — another key, another modifier, a click.
    Other,
}

/// Turns a stream of [`Signal`]s into double-taps.
///
/// A tap is the watched modifier going down and coming back up quickly with
/// nothing in between. Two clean taps inside the window are a double-tap. The
/// "nothing in between" rule is what keeps ordinary typing quiet: holding
/// Shift to reach a capital letter is a press with a key inside it, so it
/// never counts as a tap at all.
#[derive(Debug)]
pub struct Detector {
    window: Duration,
    /// When the current press started, if the modifier is down.
    down_at: Option<Instant>,
    /// When the first clean tap of a would-be pair finished.
    armed: Option<Instant>,
    /// Something else happened during this press, so it isn't a tap.
    dirty: bool,
}

impl Detector {
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            down_at: None,
            armed: None,
            dirty: false,
        }
    }

    /// Feeds one signal. Returns true on the second tap of a pair.
    pub fn feed(&mut self, signal: Signal, now: Instant) -> bool {
        match signal {
            Signal::Down => {
                // `get_or_insert` rather than a plain assignment: a repeated
                // down without an up must not restart the hold clock, or
                // holding the key could pass for a tap.
                self.down_at.get_or_insert(now);
                self.dirty = false;
                false
            }
            Signal::Other => {
                self.dirty = true;
                self.armed = None;
                false
            }
            Signal::Up => {
                let Some(start) = self.down_at.take() else {
                    // An up we never saw the down for — most likely the
                    // modifier was already held when the watcher started.
                    self.armed = None;
                    return false;
                };
                if self.dirty || now.duration_since(start) > HOLD_MAX {
                    self.armed = None;
                    return false;
                }
                match self.armed {
                    Some(first) if now.duration_since(first) <= self.window => {
                        self.armed = None;
                        true
                    }
                    // Either the first tap of a pair, or a second one that
                    // came too late — which is itself a perfectly good first
                    // tap, so start the window again from here.
                    _ => {
                        self.armed = Some(now);
                        false
                    }
                }
            }
        }
    }
}

/// Selected text, already shaped into a to-do.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Captured {
    pub title: String,
    pub body: String,
}

/// Splits a selection into a title and a description.
///
/// Returns `None` for a selection that is empty or only whitespace — there was
/// nothing selected, so the double-tap is a plain summon.
pub fn split(raw: &str) -> Option<Captured> {
    let text = raw.replace("\r\n", "\n").replace('\r', "\n");
    let text = text.trim();
    if text.is_empty() {
        return None;
    }

    let mut lines = text.split('\n');
    let first = lines.next().unwrap_or_default().trim();
    let rest = lines.collect::<Vec<_>>().join("\n");
    let rest = rest.trim();

    // Runs of spaces and tabs inside a line survive selection from rendered
    // text far more often than they mean anything, and a title is one line.
    let headline = first.split_whitespace().collect::<Vec<_>>().join(" ");

    let (title, body) = if headline.chars().count() > TITLE_MAX {
        // The first line is a paragraph. Cut the title short, but keep every
        // word of it by pushing the whole line into the description.
        let short: String = headline.chars().take(TITLE_MAX).collect();
        let short = format!("{}…", short.trim_end());
        let body = if rest.is_empty() {
            headline
        } else {
            format!("{headline}\n\n{rest}")
        };
        (short, body)
    } else {
        (headline, rest.to_string())
    };

    Some(Captured {
        title,
        body: truncate(&body, BODY_MAX),
    })
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let kept: String = s.chars().take(max).collect();
    format!("{}…", kept.trim_end())
}

/// How quick capture is configured, resolved from settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    pub key: TapKey,
    pub window: Duration,
}

/// A running listener. Dropping it stops the listener and gives back the
/// permission-shaped resources it holds.
pub struct Watcher {
    rx: Receiver<Captured>,
    inner: backend::Backend,
    config: Config,
}

impl Watcher {
    /// Starts listening. The error is meant to be shown to the user: it says
    /// what is missing, not what failed internally.
    pub fn start(config: Config) -> Result<Self, String> {
        let (rx, inner) = backend::start(config)?;
        Ok(Self { rx, inner, config })
    }

    pub fn config(&self) -> Config {
        self.config
    }

    /// The most recent capture since the last call, if any. Draining rather
    /// than taking one at a time: two double-taps between frames mean the
    /// second one is what you meant, and the UI can only act on one.
    pub fn poll(&self) -> Option<Captured> {
        let mut last = None;
        while let Ok(c) = self.rx.try_recv() {
            last = Some(c);
        }
        last
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        backend::stop(&self.inner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Duration = Duration::from_millis(400);

    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    /// Drives the detector through `(signal, ms)` pairs and reports every
    /// moment it fired.
    fn run(script: &[(Signal, u64)]) -> Vec<u64> {
        let base = Instant::now();
        let mut d = Detector::new(WINDOW);
        script
            .iter()
            .filter(|(s, ms)| d.feed(*s, at(base, *ms)))
            .map(|(_, ms)| *ms)
            .collect()
    }

    #[test]
    fn two_clean_taps_inside_the_window_fire_once() {
        use Signal::*;
        assert_eq!(
            run(&[(Down, 0), (Up, 40), (Down, 150), (Up, 190)]),
            vec![190]
        );
    }

    #[test]
    fn a_single_tap_does_nothing() {
        use Signal::*;
        assert!(run(&[(Down, 0), (Up, 40)]).is_empty());
    }

    #[test]
    fn a_second_tap_after_the_window_does_not_fire() {
        use Signal::*;
        assert!(run(&[(Down, 0), (Up, 40), (Down, 600), (Up, 640)]).is_empty());
    }

    /// The late tap is a perfectly good *first* tap, so a third one that comes
    /// quickly after it completes a pair. Otherwise a slow start would swallow
    /// the next deliberate double-tap.
    #[test]
    fn a_late_tap_starts_a_fresh_pair() {
        use Signal::*;
        assert_eq!(
            run(&[
                (Down, 0),
                (Up, 40),
                (Down, 900),
                (Up, 940),
                (Down, 1000),
                (Up, 1040),
            ]),
            vec![1040]
        );
    }

    /// The whole reason for the "nothing in between" rule: Shift+H is a press
    /// with a key inside it, so typing capitals must stay silent.
    #[test]
    fn typing_a_capital_letter_is_not_a_tap() {
        use Signal::*;
        assert!(run(&[
            (Down, 0),
            (Other, 10),
            (Up, 40),
            (Down, 150),
            (Other, 160),
            (Up, 190),
        ])
        .is_empty());
    }

    /// A capital letter typed between two otherwise clean taps must break the
    /// pair, not sit harmlessly between them.
    #[test]
    fn a_keystroke_between_two_taps_breaks_the_pair() {
        use Signal::*;
        assert!(run(&[(Down, 0), (Up, 40), (Other, 80), (Down, 150), (Up, 190)]).is_empty());
    }

    #[test]
    fn holding_the_modifier_is_not_a_tap() {
        use Signal::*;
        let held = HOLD_MAX.as_millis() as u64 + 100;
        assert!(run(&[(Down, 0), (Up, held), (Down, held + 50), (Up, held + 90)]).is_empty());
    }

    /// Auto-repeat, or any duplicated down, must not restart the hold clock —
    /// otherwise holding the key long enough looks like a tap.
    #[test]
    fn a_repeated_down_does_not_restart_the_hold_clock() {
        use Signal::*;
        let held = HOLD_MAX.as_millis() as u64 + 100;
        assert!(run(&[(Down, 0), (Down, 200), (Down, 400), (Up, held)]).is_empty());
    }

    /// The watcher can start while the key is already held, so the first thing
    /// it ever sees may be an up.
    #[test]
    fn an_unmatched_up_is_ignored() {
        use Signal::*;
        assert!(run(&[(Up, 0), (Up, 10)]).is_empty());
        assert_eq!(
            run(&[(Up, 0), (Down, 100), (Up, 140), (Down, 200), (Up, 240)]),
            vec![240]
        );
    }

    /// Four taps are two double-taps, not three overlapping ones.
    #[test]
    fn taps_are_consumed_in_pairs() {
        use Signal::*;
        assert_eq!(
            run(&[
                (Down, 0),
                (Up, 40),
                (Down, 150),
                (Up, 190),
                (Down, 300),
                (Up, 340),
                (Down, 450),
                (Up, 490),
            ]),
            vec![190, 490]
        );
    }

    #[test]
    fn a_short_single_line_is_the_whole_title() {
        let c = split("  Book the dentist  ").unwrap();
        assert_eq!(c.title, "Book the dentist");
        assert_eq!(c.body, "");
    }

    #[test]
    fn nothing_selected_is_not_a_capture() {
        assert_eq!(split(""), None);
        assert_eq!(split("   \n\t \r\n "), None);
    }

    #[test]
    fn the_first_line_titles_it_and_the_rest_describes_it() {
        let c = split("Ship the release\nCheck the changelog\nTag it").unwrap();
        assert_eq!(c.title, "Ship the release");
        assert_eq!(c.body, "Check the changelog\nTag it");
    }

    #[test]
    fn windows_and_old_mac_line_endings_are_normalised() {
        let c = split("Title\r\nBody one\rBody two").unwrap();
        assert_eq!(c.title, "Title");
        assert_eq!(c.body, "Body one\nBody two");
    }

    #[test]
    fn whitespace_inside_the_title_is_collapsed() {
        let c = split("Ship\t\t the    release").unwrap();
        assert_eq!(c.title, "Ship the release");
    }

    /// The point of cutting a long first line short is that it becomes a
    /// readable row — but not one word may be lost doing it.
    #[test]
    fn a_paragraph_keeps_every_word_it_cannot_fit_in_the_title() {
        let long = "word ".repeat(60);
        let c = split(&long).unwrap();
        assert!(c.title.chars().count() <= TITLE_MAX + 1, "{}", c.title);
        assert!(c.title.ends_with('…'));
        assert_eq!(c.body, long.trim());
    }

    #[test]
    fn a_long_first_line_keeps_the_lines_that_followed_it() {
        let text = format!("{}\nthe rest", "word ".repeat(60));
        let c = split(&text).unwrap();
        assert!(c.title.ends_with('…'));
        assert!(c.body.starts_with("word word"));
        assert!(c.body.ends_with("the rest"));
    }

    #[test]
    fn a_giant_selection_is_trimmed_to_a_sane_body() {
        let text = format!("Title\n{}", "x".repeat(BODY_MAX * 3));
        let c = split(&text).unwrap();
        assert_eq!(c.title, "Title");
        assert_eq!(c.body.chars().count(), BODY_MAX + 1);
        assert!(c.body.ends_with('…'));
    }

    /// Truncation counts characters, not bytes: cutting a multi-byte character
    /// in half would panic on a `String` slice.
    #[test]
    fn truncation_does_not_split_a_multi_byte_character() {
        let text = format!("Title\n{}", "é".repeat(BODY_MAX * 2));
        let c = split(&text).unwrap();
        assert_eq!(c.body.chars().count(), BODY_MAX + 1);
        let emoji = "🙂".repeat(TITLE_MAX * 2);
        let c = split(&emoji).unwrap();
        assert_eq!(c.title.chars().count(), TITLE_MAX + 1);
    }

    #[test]
    fn every_tap_key_has_a_distinct_label() {
        let mut seen: Vec<&str> = TapKey::ALL.iter().map(|k| k.label()).collect();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), TapKey::ALL.len());
    }

    #[test]
    fn tap_keys_round_trip_through_settings_json() {
        for key in TapKey::ALL {
            let json = serde_json::to_string(&key).unwrap();
            assert_eq!(serde_json::from_str::<TapKey>(&json).unwrap(), key);
        }
        assert_eq!(
            serde_json::from_str::<TapKey>("\"command\"").unwrap(),
            TapKey::Command
        );
    }
}
