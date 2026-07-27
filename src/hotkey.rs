//! The system-wide summon shortcut.
//!
//! On macOS `global-hotkey` registers through Carbon's `RegisterEventHotKey`,
//! which needs no Accessibility permission — so there is no first-launch
//! permission prompt.
//!
//! Registration is best-effort by design: another app may already own the
//! combination, and losing a shortcut must never stop Flodo from starting.

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

pub struct Hotkey {
    // Held for its lifetime: dropping the manager unregisters the shortcut.
    _manager: GlobalHotKeyManager,
    id: u32,
}

/// Parses a spec like `Alt+Space`, `Cmd+Shift+T`, `Ctrl+1`.
pub fn parse(spec: &str) -> Option<HotKey> {
    let mut mods = Modifiers::empty();
    let mut code = None;

    for part in spec.split('+').map(str::trim).filter(|p| !p.is_empty()) {
        match part.to_ascii_lowercase().as_str() {
            "alt" | "opt" | "option" => mods |= Modifiers::ALT,
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            "shift" => mods |= Modifiers::SHIFT,
            // HotKey::new normalises META to SUPER, so use SUPER directly.
            "cmd" | "command" | "super" | "meta" | "win" => mods |= Modifiers::SUPER,
            other => code = key_code(other),
        }
    }

    code.map(|c| HotKey::new(Some(mods), c))
}

fn key_code(name: &str) -> Option<Code> {
    Some(match name {
        "space" => Code::Space,
        "enter" | "return" => Code::Enter,
        "tab" => Code::Tab,
        "escape" | "esc" => Code::Escape,
        "backquote" | "`" => Code::Backquote,
        "a" => Code::KeyA,
        "b" => Code::KeyB,
        "c" => Code::KeyC,
        "d" => Code::KeyD,
        "e" => Code::KeyE,
        "f" => Code::KeyF,
        "g" => Code::KeyG,
        "h" => Code::KeyH,
        "i" => Code::KeyI,
        "j" => Code::KeyJ,
        "k" => Code::KeyK,
        "l" => Code::KeyL,
        "m" => Code::KeyM,
        "n" => Code::KeyN,
        "o" => Code::KeyO,
        "p" => Code::KeyP,
        "q" => Code::KeyQ,
        "r" => Code::KeyR,
        "s" => Code::KeyS,
        "t" => Code::KeyT,
        "u" => Code::KeyU,
        "v" => Code::KeyV,
        "w" => Code::KeyW,
        "x" => Code::KeyX,
        "y" => Code::KeyY,
        "z" => Code::KeyZ,
        "0" => Code::Digit0,
        "1" => Code::Digit1,
        "2" => Code::Digit2,
        "3" => Code::Digit3,
        "4" => Code::Digit4,
        "5" => Code::Digit5,
        "6" => Code::Digit6,
        "7" => Code::Digit7,
        "8" => Code::Digit8,
        "9" => Code::Digit9,
        _ => return None,
    })
}

impl Hotkey {
    /// Registers `spec`. Returns `None` (with a note on stderr) if the spec is
    /// unparsable or the combination is already taken — never panics, and never
    /// blocks startup.
    pub fn register(spec: &str) -> Option<Self> {
        let key = parse(spec).or_else(|| {
            eprintln!("flodo: could not understand hotkey {spec:?}; ignoring it");
            None
        })?;
        let manager = GlobalHotKeyManager::new()
            .map_err(|e| eprintln!("flodo: global hotkeys unavailable: {e}"))
            .ok()?;
        manager
            .register(key)
            .map_err(|e| eprintln!("flodo: could not register {spec:?} (already in use?): {e}"))
            .ok()?;
        Some(Self {
            _manager: manager,
            id: key.id(),
        })
    }

    /// True if our shortcut was pressed since the last call.
    pub fn triggered(&self) -> bool {
        let mut fired = false;
        while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.id == self.id && event.state == HotKeyState::Pressed {
                fired = true;
            }
        }
        fired
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_default_spec() {
        let k = parse("Alt+Space").unwrap();
        assert_eq!(k.mods, Modifiers::ALT);
        assert_eq!(k.key, Code::Space);
    }

    #[test]
    fn modifier_names_are_flexible_and_case_insensitive() {
        for spec in ["cmd+shift+t", "Command+Shift+T", "META+shift+T"] {
            let k = parse(spec).unwrap_or_else(|| panic!("failed on {spec}"));
            assert_eq!(k.mods, Modifiers::SUPER | Modifiers::SHIFT, "{spec}");
            assert_eq!(k.key, Code::KeyT, "{spec}");
        }
        assert_eq!(parse("opt+space").unwrap().mods, Modifiers::ALT);
        assert_eq!(parse("control+1").unwrap().key, Code::Digit1);
    }

    #[test]
    fn a_bare_key_needs_no_modifier() {
        let k = parse("F").unwrap();
        assert_eq!(k.mods, Modifiers::empty());
        assert_eq!(k.key, Code::KeyF);
    }

    /// The spec comes from a hand-editable settings file, so garbage must
    /// return None rather than panic.
    #[test]
    fn unparsable_specs_return_none_instead_of_panicking() {
        for spec in ["", "+", "Alt+", "Alt+Nonsense", "!!!", "ctrl+shift"] {
            assert!(parse(spec).is_none(), "{spec:?} should not parse");
        }
    }
}
