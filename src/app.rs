//! The application: state, the render/edit swap, and the settings sheet.

use crate::markdown::{self, FAMILY_MONO, FAMILY_UI};
use crate::model::{Store, Todo};
use crate::settings::{
    Appearance, FontChoice, Settings, FONT_SIZE_RANGE, OPACITY_RANGE, SPACING_RANGE,
};
use crate::theme::{Accent, Palette};
use crate::{fonts, hotkey, store, ui};

use eframe::egui::{self, Color32, FontFamily, FontId, Vec2};
use std::time::{Duration, Instant};

const SAVE_DEBOUNCE: Duration = Duration::from_millis(800);
const TITLE_BAR_H: f32 = 30.0;
/// Right-hand column of a row: wide enough for the description chevron and the
/// delete button that appears next to it on hover, so hovering never pushes
/// either of them over the title.
const GUTTER: f32 = 40.0;
const WINDOW_RADIUS: u8 = 12;
const ROW_RADIUS: u8 = 7;
/// How long a deleted to-do stays offered back before the toast fades.
const TOAST_TTL: Duration = Duration::from_secs(6);
const TOAST_FADE: f32 = 0.5;

/// The modifier every shortcut uses, spelled the way this platform spells it.
/// Tooltips used to say "Cmd" everywhere, which is wrong on the two platforms
/// where it isn't a key at all.
#[cfg(target_os = "macos")]
pub const MOD: &str = "Cmd";
#[cfg(not(target_os = "macos"))]
pub const MOD: &str = "Ctrl";

/// Shown in the settings sheet. The only place in the app that answers "what
/// else can I press?", which otherwise lived solely in the README.
fn shortcuts(hotkey: &str) -> Vec<(String, &'static str)> {
    let m = |k: &str| format!("{MOD}+{k}");
    vec![
        ("Enter".to_string(), "Add the to-do, keep typing"),
        (m("Enter"), "Add or edit the description"),
        (m("N"), "Jump to the composer"),
        (m("E"), "Show / hide completed"),
        (m("P"), "Pin / unpin from the top"),
        (m(","), "Settings"),
        (m("Z"), "Undo the last delete"),
        (m("Up / Down"), "Move the to-do you're editing"),
        (m("Backspace"), "Delete it"),
        ("Esc".to_string(), "Stop editing, or close this"),
        (hotkey.to_string(), "Summon Flodo from anywhere"),
    ]
}

/// A deletion you can still take back, and the only feedback that a row went
/// anywhere. Undo was previously invisible: a keystroke nobody had been told
/// about, for a row that had already vanished without comment.
struct Toast {
    message: String,
    undoable: bool,
    at: Instant,
}

/// Enough of a title to recognise it, without stretching the toast past the
/// window edge.
fn excerpt(title: &str, max: usize) -> String {
    let flat = title.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        return flat;
    }
    let kept: String = flat.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

/// Which row a drag is currently over, from the row rectangles of the previous
/// frame. Measuring the real rows — rather than assuming every row is one line
/// tall — is what lets you drag past a to-do with an open description.
fn drop_target(rects: &[Option<egui::Rect>], y: f32) -> Option<usize> {
    let known = || {
        rects
            .iter()
            .enumerate()
            .filter_map(|(i, r)| Some((i, (*r)?)))
    };
    if let Some((i, _)) = known().find(|(_, r)| y >= r.top() && y <= r.bottom()) {
        return Some(i);
    }
    // Past either end of the list: clamp to the nearest row we know about.
    let first = known().next()?;
    let last = known().next_back()?;
    if y < first.1.top() {
        Some(first.0)
    } else if y > last.1.bottom() {
        Some(last.0)
    } else {
        // In a gap between two rows: the one above owns it.
        known().rfind(|(_, r)| r.bottom() < y).map(|(i, _)| i)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Title,
    Body,
}

/// Which text field is being edited. This — not egui's focus memory — decides
/// whether a row renders markdown or shows raw source.
#[derive(Debug, Clone)]
struct Editing {
    id: u64,
    field: Field,
    draft: String,
    original: String,
    focus_requested: bool,
}

pub struct Flodo {
    store: Store,
    settings: Settings,
    notice: Option<String>,

    editing: Option<Editing>,
    composer: String,
    composer_focus: bool,
    show_settings: bool,
    show_shortcuts: bool,
    undo: Option<(usize, Todo)>,
    toast: Option<Toast>,

    dirty_todos: Option<Instant>,
    dirty_settings: Option<Instant>,

    /// Rebuilding the glyph atlas is expensive, so fonts are only reapplied
    /// when the selection actually changes.
    applied_fonts: Option<(FontChoice, FontChoice)>,
    font_list: Option<Vec<fonts::FamilyInfo>>,
    font_rx: Option<std::sync::mpsc::Receiver<Vec<fonts::FamilyInfo>>>,
    font_scan_started: bool,

    last_geometry: Option<egui::Rect>,
    hotkey: Option<hotkey::Hotkey>,
    hidden: bool,

    /// Modification time of the todo file as we last knew it. Lets us notice
    /// writes made by the CLI (or a file-sync client) instead of silently
    /// overwriting them on our next debounced save.
    disk_mtime: Option<std::time::SystemTime>,
    last_disk_check: Instant,
}

impl Flodo {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (todos, todo_notice) = store::load_todos();
        let (settings, settings_notice) = store::load_settings();

        let notice = match (&todo_notice, &settings_notice) {
            (store::LoadNotice::Quarantined(m), _) | (_, store::LoadNotice::Quarantined(m)) => {
                Some(m.clone())
            }
            _ => None,
        };

        let mut app = Self {
            store: todos,
            settings,
            notice,
            editing: None,
            composer: String::new(),
            composer_focus: false,
            show_settings: false,
            show_shortcuts: false,
            undo: None,
            toast: None,
            dirty_todos: None,
            dirty_settings: None,
            applied_fonts: None,
            font_list: None,
            font_rx: None,
            font_scan_started: false,
            last_geometry: None,
            hotkey: None,
            hidden: false,
            disk_mtime: store::todos_mtime(),
            last_disk_check: Instant::now(),
        };
        app.hotkey = hotkey::Hotkey::register(&app.settings.hotkey);
        app.seed_demo_if_requested();
        // Pay for a font scan only until a font has actually been chosen.
        // Gating on "no settings file" instead was wrong: any settings file
        // written without a font (a hand edit, or an older version) would
        // never adopt a default, and bold and italic would silently stop
        // working. Once chosen, the saved path and face index are loaded
        // directly and startup does zero enumeration.
        app.adopt_default_fonts();
        let _ = settings_notice;
        app.apply_fonts(&cc.egui_ctx);
        app
    }

    /// `FLODO_DEMO=<scenario>` seeds an in-memory list for screenshots. Because
    /// the screenshot harness captures pass 2, every state has to be reachable
    /// from the initial state rather than driven by input.
    fn seed_demo_if_requested(&mut self) {
        let Ok(scenario) = std::env::var("FLODO_DEMO") else {
            return;
        };
        self.store = Store::default();
        match scenario.as_str() {
            "empty" => {}
            "long" => {
                for i in 1..=30 {
                    let id = self.store.add(format!("Recurring thought number {i}"));
                    if i % 4 == 0 {
                        self.store.toggle(id);
                    }
                }
            }
            "settings" => {
                // Seeds are prepended, so add in reverse to read top-down.
                self.store.add("Then forget the settings exist");
                self.store.add("Pick a colour you actually like");
                self.show_settings = true;
            }
            "shortcuts" => {
                self.show_settings = true;
                self.show_shortcuts = true;
            }
            "toast" => {
                self.store.add("Reply to Sam about the offsite");
                let done = self.store.add("Renew the car registration");
                self.store.toggle(done);
                self.store.add("Pick up oat milk and coffee");
                // A delete that has already happened, so the shot shows the
                // offer rather than the row.
                self.undo = Some((1, Todo::new(1, "Book the dentist")));
                self.toast = Some(Toast {
                    message: "Deleted “Book the dentist”".into(),
                    undoable: true,
                    at: Instant::now(),
                });
            }
            // A realistic list, for the README screenshots.
            "hero" => {
                let done = self.store.add("Renew the car registration");
                self.store.toggle(done);
                self.store.add("Reply to Sam about the offsite");
                self.store.add("Pick up oat milk and coffee");
                let snippet = self.store.add("Fix the flaky `login_test`");
                if let Some(t) = self.store.get_mut(snippet) {
                    t.body = "Races on the session cookie.".into();
                }
                self.store.add("Book the dentist");
            }
            "showcase" => {
                self.store
                    .add("Everything renders in *your* font and colour");
                let a = self.store.add("Bodies hold **real** markdown");
                if let Some(t) = self.store.get_mut(a) {
                    t.expanded = true;
                    t.body = "## Headings, lists and links\n\n\
                              A body can hold a note, a \
                              [link](https://example.com), or a snippet:\n\n\
                              ```rust\nfn debounce(last: Instant) -> bool {\n    \
                              last.elapsed() > Duration::from_millis(800)\n}\n```\n\n\
                              - `inline code` stays monospace\n\
                              - lists stay tight\n  - and they nest\n\n\
                              > Long lines scroll sideways, because wrapped code \
                              is unreadable."
                        .into();
                }
            }
            "body" => {
                let c = self.store.add("Read the plan again");
                self.store.toggle(c);
                self.store.add("Keep the list scannable");
                let a = self.store.add("Ship the **markdown** body renderer");
                if let Some(t) = self.store.get_mut(a) {
                    t.expanded = true;
                    t.body = "Bodies hold *anything* — a note, a link to \
                              [the docs](https://example.com), or a snippet:\n\n\
                              ```rust\nfn debounce(last: Instant) -> bool {\n    \
                              last.elapsed() > Duration::from_millis(800)\n}\n```\n\n\
                              - `code` stays monospace\n- lists stay tight\n  - and nest\n\n\
                              > Long lines scroll sideways instead of wrapping."
                        .into();
                }
            }
            // `editing` and `rendered` are a matched pair: the same two todos,
            // shown mid-edit and after clicking away.
            "editing" | "rendered" => {
                self.store.add("Fix `login_test` before Friday");
                let a = self.store.add("Ship the **auth** refactor");
                if scenario == "editing" {
                    let raw = "Ship the **auth** refactor".to_string();
                    self.editing = Some(Editing {
                        id: a,
                        field: Field::Title,
                        draft: raw.clone(),
                        original: raw,
                        focus_requested: false,
                    });
                }
            }
            _ => {
                self.store.add("That is the whole app");
                let b = self.store.add("Give it a body for the details");
                if let Some(t) = self.store.get_mut(b) {
                    t.body = "Supports `code`, **bold**, and fenced snippets.".into();
                }
                let a = self.store.add("Add a todo and check it off");
                self.store.toggle(a);
            }
        }
    }

    fn palette(&self, ctx: &egui::Context) -> Palette {
        let system_dark = ctx.theme() == egui::Theme::Dark;
        Palette::new(self.settings.accent, self.settings.is_dark(system_dark))
    }

    fn touch_todos(&mut self) {
        self.dirty_todos = Some(Instant::now());
    }

    fn touch_settings(&mut self) {
        self.dirty_settings = Some(Instant::now());
    }

    fn apply_fonts(&mut self, ctx: &egui::Context) {
        let key = (self.settings.font.clone(), self.settings.mono_font.clone());
        if self.applied_fonts.as_ref() == Some(&key) {
            return;
        }
        ctx.set_fonts(fonts::build(&key.0, &key.1));
        self.applied_fonts = Some(key);
    }

    fn start_edit(&mut self, id: u64, field: Field) {
        let Some(todo) = self.store.get(id) else {
            return;
        };
        let original = match field {
            Field::Title => todo.title.clone(),
            Field::Body => todo.body.clone(),
        };
        self.editing = Some(Editing {
            id,
            field,
            draft: original.clone(),
            original,
            focus_requested: false,
        });
    }

    fn commit_edit(&mut self) {
        let Some(e) = self.editing.take() else { return };
        if let Some(todo) = self.store.get_mut(e.id) {
            let changed = match e.field {
                Field::Title => {
                    let v = e.draft.trim().to_string();
                    // An emptied title would leave an unclickable ghost row, so
                    // keep the previous text instead.
                    let v = if v.is_empty() { e.original.clone() } else { v };
                    let changed = todo.title != v;
                    todo.title = v;
                    changed
                }
                Field::Body => {
                    let changed = todo.body != e.draft;
                    todo.body = e.draft.clone();
                    if todo.body.trim().is_empty() {
                        todo.expanded = false;
                    }
                    changed
                }
            };
            if changed {
                self.touch_todos();
            }
        }
    }

    fn cancel_edit(&mut self) {
        let Some(e) = self.editing.take() else { return };
        // Escaping out of a description you never wrote would otherwise leave
        // the row expanded around nothing.
        if e.field == Field::Body {
            if let Some(t) = self.store.get_mut(e.id) {
                if !t.has_body() && t.expanded {
                    t.expanded = false;
                    self.touch_todos();
                }
            }
        }
    }

    fn delete(&mut self, id: u64) {
        if self.editing.as_ref().is_some_and(|e| e.id == id) {
            self.editing = None;
        }
        if let Some((ix, todo)) = self.store.remove(id) {
            self.toast = Some(Toast {
                message: format!("Deleted “{}”", excerpt(&todo.title, 24)),
                undoable: true,
                at: Instant::now(),
            });
            self.undo = Some((ix, todo));
            self.touch_todos();
        }
    }

    /// Puts back the last deleted to-do, if there is one.
    fn undo_delete(&mut self) {
        let Some((ix, todo)) = self.undo.take() else {
            return;
        };
        self.store.restore(ix, todo);
        self.touch_todos();
        self.toast = None;
    }

    fn add_from_composer(&mut self) -> Option<u64> {
        let title = self.composer.trim().to_string();
        if title.is_empty() {
            return None;
        }
        let id = self.store.add(title);
        self.composer.clear();
        // Keep focus so you can keep typing straight into the next one.
        self.composer_focus = true;
        self.touch_todos();
        Some(id)
    }

    /// Expands a to-do and puts the cursor in its description.
    fn open_body(&mut self, id: u64) {
        if let Some(t) = self.store.get_mut(id) {
            t.expanded = true;
        }
        self.touch_todos();
        self.composer_focus = false;
        self.start_edit(id, Field::Body);
    }

    /// Cmd+Enter, the one key that gets you a description from wherever you
    /// are: from the composer it adds the to-do and opens its description,
    /// from a title it jumps down into the description, and from a description
    /// it finishes and hands focus back to the composer. Adding a to-do with a
    /// note is then Enter-free typing all the way through.
    fn describe(&mut self) {
        match self.editing.as_ref().map(|e| (e.id, e.field)) {
            Some((id, Field::Title)) => {
                self.commit_edit();
                self.open_body(id);
            }
            Some((_, Field::Body)) => {
                self.commit_edit();
                self.composer_focus = true;
            }
            None => {
                if let Some(id) = self.add_from_composer() {
                    self.open_body(id);
                }
            }
        }
    }

    fn save_if_due(&mut self, ctx: &egui::Context) {
        if let Some(t) = self.dirty_todos {
            if t.elapsed() >= SAVE_DEBOUNCE {
                store::save_todos(&self.store);
                self.disk_mtime = store::todos_mtime();
                self.dirty_todos = None;
            } else {
                // egui only repaints on events, so without this a pending save
                // could sit unwritten until the next mouse move.
                ctx.request_repaint_after(SAVE_DEBOUNCE);
            }
        }
        if let Some(t) = self.dirty_settings {
            if t.elapsed() >= SAVE_DEBOUNCE {
                store::save_settings(&self.settings);
                self.dirty_settings = None;
            } else {
                ctx.request_repaint_after(SAVE_DEBOUNCE);
            }
        }
    }

    pub fn flush(&mut self) {
        if self.dirty_todos.take().is_some() {
            store::save_todos(&self.store);
            self.disk_mtime = store::todos_mtime();
        }
        if self.dirty_settings.take().is_some() {
            store::save_settings(&self.settings);
        }
    }

    /// Picks up edits made by another process. Stat-ing once a second is
    /// cheap and avoids a filesystem-watcher dependency.
    fn poll_external_changes(&mut self, ctx: &egui::Context) {
        if self.last_disk_check.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.last_disk_check = Instant::now();

        let mtime = store::todos_mtime();
        if mtime == self.disk_mtime {
            return;
        }
        self.disk_mtime = mtime;

        let Ok(disk) = store::load_todos_strict() else {
            // Unreadable on disk: keep what we have in memory rather than
            // dropping the user's list on the floor.
            return;
        };

        match self.dirty_todos {
            // Nothing unsaved locally, so the file on disk is the truth.
            None => {
                let editing = self.editing.as_ref().map(|e| e.id);
                self.store = disk;
                // Don't leave an editor open on a todo that just disappeared.
                if editing.is_some_and(|id| self.store.get(id).is_none()) {
                    self.editing = None;
                }
            }
            // We have unsaved edits, so ours win — but still adopt anything
            // new, so a `flodo add` while you were mid-sentence isn't lost.
            Some(_) => {
                let known: std::collections::HashSet<u64> =
                    self.store.todos.iter().map(|t| t.id).collect();
                for todo in disk
                    .todos
                    .into_iter()
                    .filter(|t| !known.contains(&t.id))
                    .rev()
                {
                    self.store.todos.insert(0, todo);
                }
            }
        }
        ctx.request_repaint();
    }

    fn track_geometry(&mut self, ctx: &egui::Context) {
        let Some(rect) = ctx.input(|i| i.viewport().outer_rect) else {
            return;
        };
        if self.last_geometry == Some(rect) {
            return;
        }
        self.last_geometry = Some(rect);
        self.settings.window.x = Some(rect.min.x);
        self.settings.window.y = Some(rect.min.y);
        self.settings.window.w = rect.width();
        self.settings.window.h = rect.height();
        self.touch_settings();
    }
}

// ---------------------------------------------------------------- shortcuts

impl Flodo {
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers};
        let cmd = Modifiers::COMMAND;

        // Esc: leave whatever we're in, innermost first.
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            if self.editing.is_some() {
                self.cancel_edit();
            } else if self.show_settings {
                self.show_settings = false;
            } else {
                self.toast = None;
            }
        }

        let pressed = |key: Key| ctx.input_mut(|i| i.consume_key(cmd, key));

        if pressed(Key::N) {
            self.composer_focus = true;
            self.show_settings = false;
        }
        // Consumed here rather than in the text fields, so neither the
        // composer's Enter handler nor the multiline body sees it.
        if pressed(Key::Enter) && !self.show_settings {
            self.describe();
        }
        if pressed(Key::Comma) {
            self.show_settings = !self.show_settings;
        }
        // Cmd+E, not Cmd+H: macOS reserves Cmd+H for Hide App.
        if pressed(Key::E) {
            self.settings.hide_completed = !self.settings.hide_completed;
            self.touch_settings();
        }
        if pressed(Key::P) {
            self.settings.always_on_top = !self.settings.always_on_top;
            self.apply_window_level(ctx);
            self.touch_settings();
        }
        if pressed(Key::Z) {
            self.undo_delete();
        }
        if let Some(id) = self.editing.as_ref().map(|e| e.id) {
            if pressed(Key::ArrowUp) {
                self.store.move_up(id);
                self.touch_todos();
            }
            if pressed(Key::ArrowDown) {
                self.store.move_down(id);
                self.touch_todos();
            }
            if pressed(Key::Backspace) {
                self.delete(id);
            }
        }
        if pressed(Key::W) || pressed(Key::Q) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn apply_window_level(&self, ctx: &egui::Context) {
        let level = if self.settings.always_on_top {
            egui::WindowLevel::AlwaysOnTop
        } else {
            egui::WindowLevel::Normal
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
    }
}

// ------------------------------------------------------------------ drawing

impl Flodo {
    fn title_bar(&mut self, ui: &mut egui::Ui, p: &Palette) {
        ui.horizontal(|ui| {
            ui.set_height(TITLE_BAR_H);

            // The mark doubles as the only status the app shows: how much of
            // the list is behind you.
            let total = self.store.todos.len();
            let done = self.store.todos.iter().filter(|t| t.done).count();
            let (dot, resp) = ui.allocate_exact_size(Vec2::splat(13.0), egui::Sense::hover());
            ui::progress_ring(ui.painter(), dot, done, total, p);
            resp.on_hover_text(match (done, total) {
                (_, 0) => "Nothing on the list".to_string(),
                (d, t) if d == t => format!("All {t} done"),
                (d, t) => format!("{d} of {t} done"),
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui::icon_button(ui, p, &format!("Close  ({MOD}+W)"), ui::close).clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                let open = self.show_settings;
                if ui::icon_button(
                    ui,
                    p,
                    &if open {
                        format!("Back to the list  ({MOD}+,)")
                    } else {
                        format!("Settings  ({MOD}+,)")
                    },
                    ui::sliders,
                )
                .clicked()
                {
                    self.show_settings = !open;
                }
                let pinned = self.settings.always_on_top;
                if ui::icon_button(
                    ui,
                    p,
                    &if pinned {
                        format!("Unpin from top  ({MOD}+P)")
                    } else {
                        format!("Pin on top  ({MOD}+P)")
                    },
                    |pt, r, c| ui::pin(pt, r, pinned, c),
                )
                .clicked()
                {
                    self.settings.always_on_top = !pinned;
                    self.apply_window_level(ui.ctx());
                    self.touch_settings();
                }
                if self.store.any_completed() {
                    let hidden = self.settings.hide_completed;
                    if ui::icon_button(
                        ui,
                        p,
                        &if hidden {
                            format!("Show completed  ({MOD}+E)")
                        } else {
                            format!("Hide completed  ({MOD}+E)")
                        },
                        |pt, r, c| ui::eye(pt, r, !hidden, c),
                    )
                    .clicked()
                    {
                        self.settings.hide_completed = !hidden;
                        self.touch_settings();
                    }
                }
            });
        });
    }

    /// The composer sits in its own surface so it reads as a field you can type
    /// into rather than a line of placeholder text, and lights up with the
    /// accent while it has focus.
    fn composer(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let size = self.settings.font_size;
        // `Frame::begin` rather than `Frame::show`, so the border can be
        // decided *after* the field has reported its focus — otherwise the ring
        // trails the caret by a frame.
        let mut prepared = egui::Frame::NONE
            .fill(p.surface)
            .corner_radius(9)
            .stroke(egui::Stroke::new(1.0, p.border))
            .inner_margin(egui::Margin::symmetric(8, 6))
            .begin(ui);

        let focused = {
            let ui = &mut prepared.content_ui;
            ui.horizontal(|ui| {
                let (r, plus) = ui.allocate_exact_size(Vec2::splat(ui::ICON), egui::Sense::click());
                ui::plus(ui.painter(), r, p.accent);
                if plus.clicked() {
                    self.composer_focus = true;
                }
                ui.add_space(6.0);

                let edit = egui::TextEdit::singleline(&mut self.composer)
                    .desired_width(ui.available_width())
                    .frame(egui::Frame::NONE)
                    .hint_text(
                        egui::RichText::new("New to-do")
                            .color(p.muted)
                            .size(size)
                            .family(FontFamily::Name(FAMILY_UI.into())),
                    )
                    .font(FontId::new(size, FontFamily::Name(FAMILY_UI.into())))
                    .text_color(p.text);
                let resp = ui.add(edit);

                if self.composer_focus {
                    resp.request_focus();
                    self.composer_focus = false;
                }
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    let _ = self.add_from_composer();
                }
                resp.has_focus()
            })
            .inner
        };

        if focused {
            prepared.frame.stroke = egui::Stroke::new(1.0, p.accent);
            prepared.frame.fill = p.surface_hover;
        }
        prepared.end(ui);
    }

    /// Nothing to draw is still something to say. An empty list is where the
    /// two keystrokes worth knowing get taught, and a finished one is worth
    /// marking rather than leaving as a blank panel.
    fn empty_state(&self, ui: &mut egui::Ui, p: &Palette) {
        let size = self.settings.font_size;
        let fresh = self.store.todos.is_empty();
        ui.add_space(22.0);
        ui.vertical_centered(|ui| {
            let (mark, _) = ui.allocate_exact_size(Vec2::splat(28.0), egui::Sense::hover());
            if fresh {
                ui.painter().circle_stroke(
                    mark.center(),
                    9.0,
                    egui::Stroke::new(1.5, p.muted.gamma_multiply(0.4)),
                );
            } else {
                ui::checkbox(ui.painter(), mark, 1.0, false, p);
            }
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(if fresh { "Nothing yet" } else { "All done" })
                    .color(p.text)
                    .size(size),
            );
            ui.add_space(2.0);
            for line in if fresh {
                vec![
                    "Type above and press Enter".to_string(),
                    format!("{MOD}+Enter adds a description"),
                ]
            } else {
                vec![format!("{MOD}+E brings the completed ones back")]
            } {
                ui.label(egui::RichText::new(line).color(p.muted).size(size * 0.85));
            }
        });
    }

    fn list(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let visible = self.store.visible_ids(self.settings.hide_completed);

        if visible.is_empty() {
            self.empty_state(ui, p);
            return;
        }

        let mut toggle: Option<u64> = None;
        let mut remove: Option<u64> = None;
        let mut expand: Option<u64> = None;
        let mut edit: Option<(u64, Field)> = None;
        let mut reorder: Option<(u64, usize)> = None;
        let mut draft: Option<String> = None;

        // Where every row was last frame. A drag is resolved against these
        // rather than an assumed row height, so rows with an open description
        // are dragged over accurately instead of jumping several places.
        let rects: Vec<Option<egui::Rect>> = visible
            .iter()
            .map(|id| {
                ui.ctx()
                    .read_response(egui::Id::new(("row", *id)))
                    .map(|r| r.rect)
            })
            .collect();

        for (row_ix, id) in visible.iter().copied().enumerate() {
            // Scoped so the immutable borrow of `store` ends before we write
            // the draft back below.
            let out = {
                let Some(todo) = self.store.get(id) else {
                    continue;
                };
                self.row(ui, p, todo)
            };
            if out.toggle {
                toggle = Some(id);
            }
            if out.delete {
                remove = Some(id);
            }
            if out.toggle_expand {
                expand = Some(id);
            }
            if let Some(f) = out.edit {
                edit = Some((id, f));
            }
            if let Some(y) = out.drag_y {
                if let Some(ix) = drop_target(&rects, y) {
                    if ix != row_ix {
                        reorder = Some((id, ix));
                    }
                }
            }
            if let Some(d) = out.draft {
                draft = Some(d);
            }
            ui.add_space(self.settings.spacing);
        }

        if let (Some(d), Some(e)) = (draft, self.editing.as_mut()) {
            e.draft = d;
        }

        if let Some(id) = toggle {
            self.store.toggle(id);
            self.touch_todos();
        }
        if let Some(id) = expand {
            let empty = self.store.get(id).is_some_and(|t| !t.has_body());
            if let Some(t) = self.store.get_mut(id) {
                t.expanded = !t.expanded;
            }
            self.touch_todos();
            // Opening an empty body goes straight into editing it — otherwise
            // the chevron reveals nothing at all.
            if empty {
                self.start_edit(id, Field::Body);
            }
        }
        if let Some((id, field)) = edit {
            self.commit_edit();
            self.start_edit(id, field);
        }
        if let Some(id) = remove {
            self.delete(id);
        }
        if let Some((id, to)) = reorder {
            // `to` indexes the visible list; translate back to store order.
            let target = visible.get(to).copied().unwrap_or(id);
            if let Some(ix) = self.store.index_of(target) {
                self.store.move_to(id, ix);
                self.touch_todos();
            }
        }
    }

    fn row(&self, ui: &mut egui::Ui, p: &Palette, todo: &Todo) -> RowOut {
        let mut out = RowOut::default();
        let size = self.settings.font_size;
        let editing_title = matches!(
            &self.editing,
            Some(e) if e.id == todo.id && e.field == Field::Title
        );
        let editing_body = matches!(
            &self.editing,
            Some(e) if e.id == todo.id && e.field == Field::Body
        );

        let row_id = egui::Id::new(("row", todo.id));
        // Reserved now, filled in once the row's real height is known: the
        // hover surface has to sit behind everything the row draws.
        let backdrop = ui.painter().add(egui::Shape::Noop);
        let mut dragging = false;

        let resp = ui
            .scope(|ui| {
                ui.horizontal_top(|ui| {
                    // Left gutter: drag grip on hover, then the checkbox.
                    let hovered = ui.ctx().read_response(row_id).is_some_and(|r| r.hovered());

                    let (grip_r, grip_resp) = ui.allocate_exact_size(
                        Vec2::new(10.0, ui::ICON),
                        egui::Sense::click_and_drag(),
                    );
                    dragging = grip_resp.dragged();
                    if hovered || dragging {
                        ui::grip(ui.painter(), grip_r, p.muted.gamma_multiply(0.7));
                    }
                    if dragging {
                        // The list turns the pointer into a row, from the row
                        // rectangles it measured last frame.
                        if let Some(pos) = ui.ctx().pointer_interact_pos() {
                            out.drag_y = Some(pos.y);
                        }
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                    } else if grip_resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                    }

                    let (box_r, box_resp) =
                        ui.allocate_exact_size(Vec2::splat(ui::ICON), egui::Sense::click());
                    let checked =
                        ui.ctx()
                            .animate_bool_with_time(row_id.with("done"), todo.done, 0.16);
                    ui::checkbox(ui.painter(), box_r, checked, box_resp.hovered(), p);
                    if box_resp
                        .on_hover_text(if todo.done {
                            "Mark as not done"
                        } else {
                            "Mark as done"
                        })
                        .clicked()
                    {
                        out.toggle = true;
                    }
                    ui.add_space(6.0);

                    // Reserve the right gutter up front and give the whole
                    // content column that fixed width. Otherwise the column
                    // reports the full remaining width and long body text
                    // spills past the window edge instead of wrapping.
                    let content_w = (ui.available_width() - GUTTER).max(60.0);
                    ui.allocate_ui(Vec2::new(content_w, 0.0), |ui| {
                        ui.vertical(|ui| {
                            ui.horizontal_top(|ui| {
                                ui.allocate_ui(Vec2::new(content_w, 0.0), |ui| {
                                    if editing_title {
                                        // The buffer lives in `self.editing`; the
                                        // caller writes it back after this frame.
                                        let mut draft = self
                                            .editing
                                            .as_ref()
                                            .map(|e| e.draft.clone())
                                            .unwrap_or_default();
                                        let resp = ui.add(
                                            egui::TextEdit::singleline(&mut draft)
                                                .desired_width(f32::INFINITY)
                                                .frame(egui::Frame::NONE)
                                                .font(FontId::new(
                                                    size,
                                                    FontFamily::Name(FAMILY_UI.into()),
                                                ))
                                                .text_color(p.text)
                                                .id(egui::Id::new(("title", todo.id))),
                                        );
                                        let _ = &resp;
                                        out.draft = Some(draft);
                                    } else {
                                        let ctx = markdown::Ctx {
                                            palette: p,
                                            size,
                                            dim: todo.done,
                                        };
                                        let mut job = markdown::inline_job(&todo.title, &ctx);
                                        if todo.done {
                                            for s in &mut job.sections {
                                                s.format.strikethrough =
                                                    egui::Stroke::new(1.0, p.muted);
                                            }
                                        }
                                        job.wrap.max_width = ui.available_width();
                                        if ui
                                            .add(
                                                egui::Label::new(job)
                                                    .sense(egui::Sense::click())
                                                    .selectable(false),
                                            )
                                            .clicked()
                                        {
                                            out.edit = Some(Field::Title);
                                        }
                                    }
                                });
                            });

                            if todo.expanded {
                                ui.add_space(2.0);
                                if editing_body {
                                    let mut draft = self
                                        .editing
                                        .as_ref()
                                        .map(|e| e.draft.clone())
                                        .unwrap_or_default();
                                    let rows = draft.lines().count().clamp(2, 20);
                                    let resp = ui.add(
                                        egui::TextEdit::multiline(&mut draft)
                                            .desired_width(f32::INFINITY)
                                            .desired_rows(rows)
                                            .frame(egui::Frame::NONE)
                                            .font(FontId::new(
                                                size - 1.0,
                                                FontFamily::Name(FAMILY_MONO.into()),
                                            ))
                                            .text_color(p.text)
                                            .hint_text(
                                                egui::RichText::new("Notes, links, code…")
                                                    .color(p.muted),
                                            )
                                            .id(egui::Id::new(("body", todo.id))),
                                    );
                                    let _ = &resp;
                                    out.draft = Some(draft);
                                } else if todo.has_body() {
                                    let ctx = markdown::Ctx {
                                        palette: p,
                                        size,
                                        dim: todo.done,
                                    };
                                    let blocks = markdown::parse(&todo.body);
                                    let resp = ui
                                        .scope(|ui| {
                                            markdown::render(ui, &blocks, &ctx, todo.id, content_w);
                                        })
                                        .response;
                                    if resp.interact(egui::Sense::click()).clicked() {
                                        out.edit = Some(Field::Body);
                                    }
                                }
                                ui.add_space(2.0);
                            }
                        });
                    });

                    // Right gutter: the description chevron, delete on hover.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                        if hovered
                            && ui::icon_button(
                                ui,
                                p,
                                &format!("Delete  ({MOD}+Backspace)"),
                                ui::close,
                            )
                            .clicked()
                        {
                            out.delete = true;
                        }
                        // Shown on every row, not just on hover: a chevron you
                        // have to find by sweeping the mouse over the list is
                        // not a way to discover that descriptions exist.
                        let has_body = todo.has_body();
                        let (r, resp) =
                            ui.allocate_exact_size(Vec2::splat(ui::ICON), egui::Sense::click());
                        let c = if resp.hovered() {
                            p.text
                        } else if has_body {
                            p.muted
                        } else {
                            // Faint, so an empty row still reads as one line.
                            p.muted.gamma_multiply(0.4)
                        };
                        ui::chevron(ui.painter(), r, todo.expanded, c);
                        if resp
                            .on_hover_text(if has_body {
                                "Show / hide the description".to_string()
                            } else {
                                format!("Add a description  ({MOD}+Enter)")
                            })
                            .clicked()
                        {
                            out.toggle_expand = true;
                        }
                    });
                });
            })
            .response;

        // The hover surface. It makes the row one target rather than a line of
        // text with two icons floating near it, and gives the delete button and
        // the grip something to appear *on* instead of out of nowhere.
        let lit = ui.ctx().read_response(row_id).is_some_and(|r| r.hovered()) || dragging;
        let active = editing_title || editing_body;
        let t = ui
            .ctx()
            .animate_bool_with_time(row_id.with("lit"), lit || active, 0.10);
        if t > 0.01 {
            let rect = resp.rect.expand2(Vec2::new(5.0, 3.0));
            let fill = if active { p.surface } else { p.surface_hover };
            ui.painter().set(
                backdrop,
                egui::epaint::RectShape::filled(rect, ROW_RADIUS, fill.gamma_multiply(t)),
            );
            if active {
                // A quiet accent edge on whatever you're typing into, so a long
                // list never leaves you hunting for the caret.
                let bar = egui::Rect::from_min_size(
                    rect.left_top() + Vec2::new(0.0, 2.0),
                    Vec2::new(2.0, (rect.height() - 4.0).max(0.0)),
                );
                ui.painter().rect_filled(bar, 1, p.accent.gamma_multiply(t));
            }
        }

        // Register the row rect under a stable id so the *next* frame knows
        // whether it is hovered (immediate mode has no persistent widgets).
        ui.interact(resp.rect, row_id, egui::Sense::hover());

        out
    }
}

/// What a single row reported this frame. A struct rather than an enum because
/// a row can do more than one thing at once (edit text *and* be dragged).
#[derive(Default)]
struct RowOut {
    toggle: bool,
    delete: bool,
    toggle_expand: bool,
    edit: Option<Field>,
    /// Where the pointer is while this row's grip is being dragged. The list
    /// owns the translation from a y to a position.
    drag_y: Option<f32>,
    draft: Option<String>,
}

// ------------------------------------------------------------------ settings

impl Flodo {
    fn settings_sheet(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let size = self.settings.font_size;
        let label = |ui: &mut egui::Ui, text: &str| {
            ui.label(
                egui::RichText::new(text)
                    .color(p.muted)
                    .size(size * 0.9)
                    .family(FontFamily::Name(FAMILY_UI.into())),
            );
        };
        // Every slider gets its value spelled out on the right. Three unlabelled
        // tracks left you dragging to find out what you had.
        let labelled = |ui: &mut egui::Ui, text: &str, value: String| {
            ui.horizontal(|ui| {
                label(ui, text);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(value)
                            .color(p.muted.gamma_multiply(0.85))
                            .size(size * 0.85),
                    );
                });
            });
        };

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Settings").color(p.text).size(size));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui::text_button(ui, p, "Done", size * 0.9).clicked() {
                    self.show_settings = false;
                }
            });
        });
        ui.add_space(4.0);
        ui::rule(ui, p);
        ui.add_space(6.0);

        egui::ScrollArea::vertical()
            .id_salt("settings")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 10.0;

                // Accent swatches.
                label(ui, "Colour");
                ui.horizontal_wrapped(|ui| {
                    for accent in Accent::ALL {
                        let chosen = self.settings.accent == accent;
                        let (r, resp) =
                            ui.allocate_exact_size(Vec2::splat(22.0), egui::Sense::click());
                        let dot = Palette::new(accent, p.dark).accent;
                        // Growing under the cursor makes eight small circles
                        // feel like eight buttons.
                        let grow = ui.ctx().animate_bool_with_time(
                            egui::Id::new(("swatch", accent)),
                            resp.hovered(),
                            0.09,
                        );
                        ui.painter().circle_filled(r.center(), 8.0 + grow, dot);
                        if chosen {
                            ui.painter().circle_stroke(
                                r.center(),
                                10.5,
                                egui::Stroke::new(1.5, dot),
                            );
                        }
                        if resp.on_hover_text(accent.label()).clicked() {
                            self.settings.accent = accent;
                            self.touch_settings();
                        }
                    }
                });

                label(ui, "Appearance");
                ui.horizontal(|ui| {
                    for mode in Appearance::ALL {
                        let on = self.settings.appearance == mode;
                        if ui
                            .selectable_label(
                                on,
                                egui::RichText::new(mode.label())
                                    .size(size * 0.9)
                                    .color(if on { p.text } else { p.muted }),
                            )
                            .clicked()
                        {
                            self.settings.appearance = mode;
                            self.touch_settings();
                        }
                    }
                });

                self.font_picker(ui, p, "Font", false);
                self.font_picker(ui, p, "Code font", true);

                labelled(
                    ui,
                    "Text size",
                    format!("{:.0} px", self.settings.font_size),
                );
                if ui
                    .add(
                        egui::Slider::new(
                            &mut self.settings.font_size,
                            FONT_SIZE_RANGE.0..=FONT_SIZE_RANGE.1,
                        )
                        .show_value(false),
                    )
                    .changed()
                {
                    self.touch_settings();
                }

                labelled(
                    ui,
                    "Row spacing",
                    format!("{:.0} px", self.settings.spacing),
                );
                if ui
                    .add(
                        egui::Slider::new(
                            &mut self.settings.spacing,
                            SPACING_RANGE.0..=SPACING_RANGE.1,
                        )
                        .show_value(false),
                    )
                    .changed()
                {
                    self.touch_settings();
                }

                labelled(
                    ui,
                    "Opacity",
                    format!("{:.0}%", self.settings.opacity * 100.0),
                );
                if ui
                    .add(
                        egui::Slider::new(
                            &mut self.settings.opacity,
                            OPACITY_RANGE.0..=OPACITY_RANGE.1,
                        )
                        .show_value(false),
                    )
                    .changed()
                {
                    self.touch_settings();
                }

                ui.add_space(2.0);
                ui::rule(ui, p);
                self.shortcut_list(ui, p);
            });
    }

    /// The shortcuts, folded away. Collapsed it costs one line, so the sheet is
    /// still the same short screen; open it is the answer to "what else can I
    /// press?" without leaving the app for the README.
    fn shortcut_list(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let size = self.settings.font_size;
        let open = self.show_shortcuts;

        let head = ui
            .horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(ui::ICON), egui::Sense::hover());
                ui::chevron(ui.painter(), r, open, p.muted);
                ui.label(
                    egui::RichText::new("Keyboard shortcuts")
                        .color(p.muted)
                        .size(size * 0.9),
                );
            })
            .response;
        let head = head.interact(egui::Sense::click());
        if head.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if head.clicked() {
            self.show_shortcuts = !open;
        }
        if !open {
            return;
        }

        ui.spacing_mut().item_spacing.y = 4.0;
        for (keys, what) in shortcuts(&self.settings.hotkey) {
            ui.horizontal(|ui| {
                ui.add_space(ui::ICON);
                // A fixed key column, so the descriptions line up into a table
                // instead of stepping in and out with the length of the keys.
                ui.scope(|ui| {
                    ui.set_min_width(116.0);
                    ui.label(
                        egui::RichText::new(keys)
                            .color(p.text.gamma_multiply(0.9))
                            .size(size * 0.8)
                            .family(FontFamily::Name(FAMILY_MONO.into())),
                    );
                });
                ui.label(egui::RichText::new(what).color(p.muted).size(size * 0.8));
            });
        }
    }

    fn font_picker(&mut self, ui: &mut egui::Ui, p: &Palette, title: &str, mono: bool) {
        let size = self.settings.font_size;
        ui.label(
            egui::RichText::new(title)
                .color(p.muted)
                .size(size * 0.9)
                .family(FontFamily::Name(FAMILY_UI.into())),
        );

        let current = if mono {
            self.settings.mono_font.label().to_string()
        } else {
            self.settings.font.label().to_string()
        };

        egui::ComboBox::from_id_salt(("font", mono))
            .selected_text(egui::RichText::new(current).size(size * 0.9).color(p.text))
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                // Enumeration is slow, so it only starts once the picker opens
                // and runs off the UI thread.
                self.ensure_font_scan(ui.ctx());

                let mut pick: Option<FontChoice> = None;
                if ui.selectable_label(false, "Built-in").clicked() {
                    pick = Some(FontChoice::default());
                }
                match &self.font_list {
                    None => {
                        ui.label(egui::RichText::new("Scanning fonts…").color(p.muted));
                    }
                    Some(list) => {
                        for f in list.iter().filter(|f| !mono || f.monospaced) {
                            if ui.selectable_label(false, &f.name).clicked() {
                                pick = Some(FontChoice {
                                    family: f.name.clone(),
                                    path: f.path.clone(),
                                    index: f.index,
                                });
                            }
                        }
                    }
                }
                if let Some(choice) = pick {
                    if mono {
                        self.settings.mono_font = choice;
                    } else {
                        self.settings.font = choice;
                    }
                    // Even picking "Built-in" is a choice, and must not be
                    // overridden by default adoption on the next launch.
                    self.settings.fonts_chosen = true;
                    self.touch_settings();
                }
            });
    }

    /// Fills in fonts the user has never chosen. A deliberate "Built-in"
    /// selection sets `fonts_chosen`, so it is left alone.
    fn adopt_defaults_from(&mut self, list: &[fonts::FamilyInfo]) {
        if self.settings.fonts_chosen {
            return;
        }
        let pick = |f: fonts::FamilyInfo| FontChoice {
            family: f.name,
            path: f.path,
            index: f.index,
        };
        if self.settings.font.is_default() {
            if let Some(f) = fonts::default_proportional(list) {
                self.settings.font = pick(f);
            }
        }
        if self.settings.mono_font.is_default() {
            if let Some(f) = fonts::default_mono(list) {
                self.settings.mono_font = pick(f);
            }
        }
        self.settings.fonts_chosen = true;
        self.touch_settings();
    }

    fn adopt_default_fonts(&mut self) {
        if self.settings.fonts_chosen {
            return;
        }
        let list = fonts::enumerate();
        self.adopt_defaults_from(&list);
        self.font_list = Some(list);
        self.font_scan_started = true;
    }

    /// The hotkey fires on an OS thread, so the UI has to be woken to notice.
    /// Repainting on a slow timer is enough and costs nothing while idle.
    fn poll_hotkey(&mut self, ctx: &egui::Context) {
        let Some(hk) = &self.hotkey else { return };
        if hk.triggered() {
            self.hidden = !self.hidden;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(!self.hidden));
            if !self.hidden {
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                self.composer_focus = true;
            }
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    }

    fn ensure_font_scan(&mut self, ctx: &egui::Context) {
        if self.font_scan_started {
            return;
        }
        self.font_scan_started = true;
        let (tx, rx) = std::sync::mpsc::channel();
        self.font_rx = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let list = fonts::enumerate();
            let _ = tx.send(list);
            ctx.request_repaint();
        });
    }

    fn poll_font_scan(&mut self) {
        if let Some(rx) = &self.font_rx {
            if let Ok(list) = rx.try_recv() {
                self.adopt_defaults_from(&list);
                self.font_list = Some(list);
                self.font_rx = None;
            }
        }
    }
}

// --------------------------------------------------------------- eframe glue

impl eframe::App for Flodo {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.flush();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_hotkey(&ctx);
        self.poll_external_changes(&ctx);
        self.poll_font_scan();
        self.apply_fonts(&ctx);
        self.track_geometry(&ctx);

        let p = self.palette(&ctx);
        let size = self.settings.font_size;
        // Styles are per-theme in 0.35; write both so the palette applies
        // whichever theme the OS reports.
        ctx.all_styles_mut(|style| {
            p.apply(style);
            style.spacing.item_spacing.y = 4.0;
            // Anything drawn without an explicit font — combo boxes, tooltips,
            // selectable labels — used to fall back to the bundled face at a
            // fixed size, so half the app ignored both font pickers and the
            // text-size slider.
            let ui_family = FontFamily::Name(FAMILY_UI.into());
            let mono_family = FontFamily::Name(FAMILY_MONO.into());
            style.text_styles = [
                (
                    egui::TextStyle::Small,
                    FontId::new(size * 0.85, ui_family.clone()),
                ),
                (egui::TextStyle::Body, FontId::new(size, ui_family.clone())),
                (
                    egui::TextStyle::Button,
                    FontId::new(size * 0.9, ui_family.clone()),
                ),
                (
                    egui::TextStyle::Heading,
                    FontId::new(size * 1.25, ui_family),
                ),
                (
                    egui::TextStyle::Monospace,
                    FontId::new(size - 1.0, mono_family),
                ),
            ]
            .into();
        });

        self.handle_shortcuts(&ctx);

        // Opacity applies to our own fill, not the window, so text stays crisp.
        let fill = if self.settings.opacity >= 0.999 {
            p.bg
        } else {
            Color32::from_rgba_unmultiplied(
                p.bg.r(),
                p.bg.g(),
                p.bg.b(),
                (self.settings.opacity * 255.0) as u8,
            )
        };

        let frame = egui::Frame::NONE
            .fill(fill)
            .corner_radius(WINDOW_RADIUS)
            .stroke(egui::Stroke::new(1.0, p.border))
            .inner_margin(egui::Margin::symmetric(10, 8));

        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
            // Allocated first so widgets drawn later win the pointer and only
            // background starts a window drag.
            //
            // Winning the drag hit *is* the test for that: egui hands a drag
            // to the topmost widget that senses one, so a text field or a
            // button under the cursor takes it instead. Guarding on
            // `egui_wants_pointer_input()` as well never worked — it is true
            // whenever a widget is being pressed, which by the frame a drag is
            // reported includes this one, so the window never moved at all.
            let bg = ui.interact(
                ui.available_rect_before_wrap(),
                egui::Id::new("bg-drag"),
                egui::Sense::click_and_drag(),
            );
            if bg.drag_started_by(egui::PointerButton::Primary) {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }

            self.title_bar(ui, &p);

            if let Some(msg) = self.notice.clone() {
                egui::Frame::NONE
                    .fill(p.surface)
                    .corner_radius(8)
                    .stroke(egui::Stroke::new(1.0, p.accent.gamma_multiply(0.5)))
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let w = (ui.available_width() - ui::ICON - 10.0).max(60.0);
                            ui.allocate_ui(Vec2::new(w, 0.0), |ui| {
                                ui.label(egui::RichText::new(&msg).color(p.text).size(size * 0.85));
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui::icon_button(ui, &p, "Dismiss", ui::close).clicked() {
                                        self.notice = None;
                                    }
                                },
                            );
                        });
                    });
                ui.add_space(6.0);
            }

            if self.show_settings {
                self.settings_sheet(ui, &p);
                return;
            }

            // Composer on top: new todos are prepended, so pressing Enter puts
            // the todo on the line directly below where you typed it and
            // leaves the field focused for the next one.
            self.composer(ui, &p);
            ui.add_space(6.0);

            egui::ScrollArea::vertical()
                .id_salt("list")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    self.list(ui, &p);
                });
        });

        self.toast_ui(&ctx, &p);

        // Drafts are written back after drawing so the row closure can stay
        // an immutable borrow of the store.
        self.settle_edits(&ctx);
        self.save_if_due(&ctx);
    }
}

impl Flodo {
    /// The one piece of transient feedback the app has: what just went, and the
    /// offer to put it back. Floats over the bottom of the list so it never
    /// reflows the rows underneath it, and fades out on its own.
    fn toast_ui(&mut self, ctx: &egui::Context, p: &Palette) {
        let Some(toast) = &self.toast else { return };
        let age = toast.at.elapsed();
        if age >= TOAST_TTL {
            self.toast = None;
            ctx.request_repaint();
            return;
        }
        let left = (TOAST_TTL - age).as_secs_f32();
        let alpha = (left / TOAST_FADE).min(1.0);
        // Keep repainting while it is on screen: without input, egui would
        // otherwise leave it frozen there until the next mouse move.
        ctx.request_repaint_after(Duration::from_millis(if left > TOAST_FADE {
            100
        } else {
            16
        }));

        let size = self.settings.font_size;
        let message = toast.message.clone();
        let undoable = toast.undoable && self.undo.is_some();
        let mut undo = false;
        let mut dismiss = false;

        egui::Area::new(egui::Id::new("toast"))
            .anchor(egui::Align2::CENTER_BOTTOM, Vec2::new(0.0, -12.0))
            .order(egui::Order::Foreground)
            .interactable(true)
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(p.surface.gamma_multiply(alpha))
                    .corner_radius(10)
                    .stroke(egui::Stroke::new(1.0, p.border.gamma_multiply(alpha)))
                    .inner_margin(egui::Margin::symmetric(10, 7))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(&message)
                                    .color(p.text.gamma_multiply(alpha))
                                    .size(size * 0.85),
                            );
                            if undoable {
                                ui.add_space(8.0);
                                if ui::text_button(ui, p, "Undo", size * 0.85).clicked() {
                                    undo = true;
                                }
                            }
                            ui.add_space(2.0);
                            if ui::icon_button(ui, p, "Dismiss", ui::close).clicked() {
                                dismiss = true;
                            }
                        });
                    });
            });

        if undo {
            self.undo_delete();
        } else if dismiss {
            self.toast = None;
        }
    }

    /// Applies focus requests and commits on focus loss.
    fn settle_edits(&mut self, ctx: &egui::Context) {
        let Some(e) = self.editing.as_mut() else {
            return;
        };
        let id = match e.field {
            Field::Title => egui::Id::new(("title", e.id)),
            Field::Body => egui::Id::new(("body", e.id)),
        };
        if !e.focus_requested {
            ctx.memory_mut(|m| m.request_focus(id));
            e.focus_requested = true;
            return;
        }
        // Once focus has been granted and then lost, the edit is done.
        let has_focus = ctx.memory(|m| m.has_focus(id));
        if !has_focus {
            self.commit_edit();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Pos2, Rect};

    fn rows(heights: &[f32]) -> Vec<Option<Rect>> {
        let mut y = 0.0;
        heights
            .iter()
            .map(|h| {
                let r = Rect::from_min_max(Pos2::new(0.0, y), Pos2::new(100.0, y + h));
                y += h + 4.0;
                Some(r)
            })
            .collect()
    }

    #[test]
    fn a_drag_lands_on_the_row_under_the_pointer() {
        let r = rows(&[20.0, 20.0, 20.0]);
        assert_eq!(drop_target(&r, 5.0), Some(0));
        assert_eq!(drop_target(&r, 30.0), Some(1));
        assert_eq!(drop_target(&r, 55.0), Some(2));
    }

    /// The old arithmetic assumed every row was one line tall, so dragging past
    /// a to-do with an open description overshot by however many lines it had.
    #[test]
    fn tall_rows_count_once_however_tall_they_are() {
        let r = rows(&[20.0, 120.0, 20.0]);
        assert_eq!(drop_target(&r, 100.0), Some(1), "still inside the tall row");
        assert_eq!(drop_target(&r, 150.0), Some(2));
    }

    #[test]
    fn dragging_past_either_end_clamps_to_the_list() {
        let r = rows(&[20.0, 20.0]);
        assert_eq!(drop_target(&r, -500.0), Some(0));
        assert_eq!(drop_target(&r, 5000.0), Some(1));
        assert_eq!(drop_target(&[], 10.0), None);
    }

    #[test]
    fn the_gap_between_two_rows_belongs_to_the_one_above() {
        let r = rows(&[20.0, 20.0]);
        assert_eq!(drop_target(&r, 22.0), Some(0));
    }

    #[test]
    fn an_excerpt_keeps_short_titles_whole() {
        assert_eq!(excerpt("Buy oat milk", 24), "Buy oat milk");
        assert_eq!(excerpt("  spaced   out\nnote ", 24), "spaced out note");
    }

    #[test]
    fn an_excerpt_truncates_without_splitting_a_char() {
        let long = "🙂".repeat(40);
        let out = excerpt(&long, 10);
        assert_eq!(out.chars().count(), 10, "9 kept plus the ellipsis");
        assert!(out.ends_with('…'));
    }

    #[test]
    fn every_shortcut_is_spelled_for_this_platform() {
        let list = shortcuts("Alt+Space");
        assert!(list.iter().any(|(k, _)| k == "Alt+Space"));
        assert!(list.iter().any(|(k, _)| k == &format!("{MOD}+N")));
        assert!(
            !list.iter().any(|(k, _)| k.contains("Cmd") && MOD != "Cmd"),
            "no Cmd off macOS"
        );
    }
}
