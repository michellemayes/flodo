//! Everywhere that isn't macOS.
//!
//! Hearing a bare modifier and reading another app's selection are both
//! per-platform problems with no shared answer: X11 needs XRecord, Wayland
//! refuses by design and routes through a portal, and Windows wants a
//! low-level keyboard hook. Rather than half-answer them, quick capture says
//! so plainly and the rest of Flodo — including the [`crate::hotkey`] summon,
//! which works everywhere — carries on unchanged.

use super::{Captured, Config};
use std::sync::mpsc::Receiver;

pub struct Backend;

pub fn start(_config: Config) -> Result<(Receiver<Captured>, Backend), String> {
    Err("Quick capture is macOS-only for now.".to_string())
}

pub fn stop(_backend: &Backend) {}
