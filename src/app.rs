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
const GUTTER: f32 = 22.0;
const WINDOW_RADIUS: u8 = 12;

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
    undo: Option<(usize, Todo)>,

    dirty_todos: Option<Instant>,
    dirty_settings: Option<Instant>,

    /// Rebuilding the glyph atlas is expensive, so fonts are only reapplied
    /// when the selection actually changes.
    applied_fonts: Option<(FontChoice, FontChoice)>,
    font_list: Option<Vec<fonts::FamilyInfo>>,
    font_rx: Option<std::sync::mpsc::Receiver<Vec<fonts::FamilyInfo>>>,
    font_scan_started: bool,

    last_geometry: Option<egui::Rect>,
    pending_scroll: Option<u64>,
    hotkey: Option<hotkey::Hotkey>,
    hidden: bool,
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
            undo: None,
            dirty_todos: None,
            dirty_settings: None,
            applied_fonts: None,
            font_list: None,
            font_rx: None,
            font_scan_started: false,
            last_geometry: None,
            pending_scroll: None,
            hotkey: None,
            hidden: false,
        };
        app.hotkey = hotkey::Hotkey::register(&app.settings.hotkey);
        app.seed_demo_if_requested();
        // Only on a genuinely first run (no settings file yet) do we pay for a
        // synchronous font scan. Every later launch loads the saved path and
        // index directly, so startup does zero enumeration.
        if settings_notice == store::LoadNotice::Fresh {
            app.adopt_default_fonts();
        }
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
                self.store.add("Pick a colour you actually like");
                self.store.add("Then forget the settings exist");
                self.show_settings = true;
            }
            "body" => {
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
                self.store.add("Keep the list scannable");
                let c = self.store.add("Read the plan again");
                self.store.toggle(c);
            }
            "editing" => {
                let a = self.store.add("Click a title to edit it");
                self.store.add("Raw markdown shows while you type");
                self.editing = Some(Editing {
                    id: a,
                    field: Field::Title,
                    draft: "Click a title to **edit** it".into(),
                    original: "Click a title to edit it".into(),
                    focus_requested: false,
                });
            }
            _ => {
                let a = self.store.add("Add a todo and check it off");
                let b = self.store.add("Give it a body for the details");
                if let Some(t) = self.store.get_mut(b) {
                    t.body = "Supports `code`, **bold**, and fenced snippets.".into();
                }
                self.store.add("That is the whole app");
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
        self.editing = None;
    }

    fn delete(&mut self, id: u64) {
        if self.editing.as_ref().is_some_and(|e| e.id == id) {
            self.editing = None;
        }
        if let Some((ix, todo)) = self.store.remove(id) {
            self.undo = Some((ix, todo));
            self.touch_todos();
        }
    }

    fn add_from_composer(&mut self) {
        let title = self.composer.trim().to_string();
        if title.is_empty() {
            return;
        }
        let id = self.store.add(title);
        self.composer.clear();
        self.composer_focus = true;
        self.pending_scroll = Some(id);
        self.touch_todos();
    }

    fn save_if_due(&mut self, ctx: &egui::Context) {
        if let Some(t) = self.dirty_todos {
            if t.elapsed() >= SAVE_DEBOUNCE {
                store::save_todos(&self.store);
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
        }
        if self.dirty_settings.take().is_some() {
            store::save_settings(&self.settings);
        }
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
            }
        }

        let pressed = |key: Key| ctx.input_mut(|i| i.consume_key(cmd, key));

        if pressed(Key::N) {
            self.composer_focus = true;
            self.show_settings = false;
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
            if let Some((ix, todo)) = self.undo.take() {
                self.store.restore(ix, todo);
                self.touch_todos();
            }
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

            let (dot, _) = ui.allocate_exact_size(Vec2::splat(9.0), egui::Sense::hover());
            ui.painter().circle_filled(dot.center(), 4.0, p.accent);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui::icon_button(ui, p, "Close  (Cmd+W)", ui::close).clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if ui::icon_button(ui, p, "Settings  (Cmd+,)", ui::sliders).clicked() {
                    self.show_settings = !self.show_settings;
                }
                let pinned = self.settings.always_on_top;
                if ui::icon_button(
                    ui,
                    p,
                    if pinned {
                        "Unpin from top  (Cmd+P)"
                    } else {
                        "Pin on top  (Cmd+P)"
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
                        if hidden {
                            "Show completed  (Cmd+E)"
                        } else {
                            "Hide completed  (Cmd+E)"
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

    fn composer(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let size = self.settings.font_size;
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(ui::ICON), egui::Sense::hover());
            ui::plus(ui.painter(), r, p.accent);
            ui.add_space(6.0);

            let edit = egui::TextEdit::singleline(&mut self.composer)
                .desired_width(ui.available_width())
                .frame(egui::Frame::NONE)
                .hint_text(
                    egui::RichText::new("New todo")
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
                self.add_from_composer();
            }
        });
    }

    fn list(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let visible = self.store.visible_ids(self.settings.hide_completed);

        if visible.is_empty() {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new(if self.store.todos.is_empty() {
                        "Nothing yet."
                    } else {
                        "All done."
                    })
                    .color(p.muted)
                    .size(self.settings.font_size),
                );
            });
            return;
        }

        let mut toggle: Option<u64> = None;
        let mut remove: Option<u64> = None;
        let mut expand: Option<u64> = None;
        let mut edit: Option<(u64, Field)> = None;
        let mut reorder: Option<(u64, usize)> = None;
        let mut draft: Option<String> = None;

        for (row_ix, id) in visible.iter().copied().enumerate() {
            // Scoped so the immutable borrow of `store` ends before we write
            // the draft back below.
            let out = {
                let Some(todo) = self.store.get(id) else {
                    continue;
                };
                self.row(ui, p, todo, row_ix)
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
            if let Some(ix) = out.drop_at {
                reorder = Some((id, ix));
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

    fn row(&self, ui: &mut egui::Ui, p: &Palette, todo: &Todo, row_ix: usize) -> RowOut {
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
        let resp = ui
            .scope(|ui| {
                ui.horizontal_top(|ui| {
                    // Left gutter: drag grip on hover, then the checkbox.
                    let hovered = ui.ctx().read_response(row_id).is_some_and(|r| r.hovered());

                    let (grip_r, grip_resp) = ui.allocate_exact_size(
                        Vec2::new(10.0, ui::ICON),
                        egui::Sense::click_and_drag(),
                    );
                    if hovered || grip_resp.dragged() {
                        ui::grip(ui.painter(), grip_r, p.muted.gamma_multiply(0.7));
                    }
                    if grip_resp.dragged() {
                        // Translate the pointer's y into a row index.
                        if let Some(pos) = ui.ctx().pointer_interact_pos() {
                            let row_h = ui::ICON + self.settings.spacing + 6.0;
                            let delta = (pos.y - grip_r.center().y) / row_h;
                            let target = (row_ix as f32 + delta).round().max(0.0) as usize;
                            if target != row_ix {
                                out.drop_at = Some(target);
                            }
                        }
                    }
                    if grip_resp.dragged() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                    } else if grip_resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                    }

                    let (box_r, box_resp) =
                        ui.allocate_exact_size(Vec2::splat(ui::ICON), egui::Sense::click());
                    ui::checkbox(ui.painter(), box_r, todo.done, box_resp.hovered(), p);
                    if box_resp.clicked() {
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

                    // Right gutter: chevron when there's a body, delete on hover.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                        if hovered
                            && ui::icon_button(ui, p, "Delete  (Cmd+Backspace)", |pt, r, c| {
                                ui::close(pt, r, c)
                            })
                            .clicked()
                        {
                            out.delete = true;
                        }
                        if todo.has_body() || hovered {
                            let (r, resp) =
                                ui.allocate_exact_size(Vec2::splat(ui::ICON), egui::Sense::click());
                            let c = if todo.has_body() { p.muted } else { p.border };
                            ui::chevron(ui.painter(), r, todo.expanded, c);
                            if resp.clicked() {
                                out.toggle_expand = true;
                            }
                        }
                    });
                });
            })
            .response;

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
    drop_at: Option<usize>,
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

        egui::ScrollArea::vertical()
            .id_salt("settings")
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
                        ui.painter().circle_filled(r.center(), 8.0, dot);
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

                label(ui, "Text size");
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

                label(ui, "Row spacing");
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

                label(ui, "Opacity");
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

                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(format!("Summon with {}", self.settings.hotkey))
                        .color(p.muted)
                        .size(size * 0.8),
                );
            });
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
                    self.touch_settings();
                }
            });
    }

    /// Fills in any font the user hasn't explicitly chosen.
    fn adopt_defaults_from(&mut self, list: &[fonts::FamilyInfo]) {
        let pick = |f: fonts::FamilyInfo| FontChoice {
            family: f.name,
            path: f.path,
            index: f.index,
        };
        if self.settings.font.is_default() {
            if let Some(f) = fonts::default_proportional(list) {
                self.settings.font = pick(f);
                self.touch_settings();
            }
        }
        if self.settings.mono_font.is_default() {
            if let Some(f) = fonts::default_mono(list) {
                self.settings.mono_font = pick(f);
                self.touch_settings();
            }
        }
    }

    fn adopt_default_fonts(&mut self) {
        if !self.settings.font.is_default() && !self.settings.mono_font.is_default() {
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
        self.poll_font_scan();
        self.apply_fonts(&ctx);
        self.track_geometry(&ctx);

        let p = self.palette(&ctx);
        // Styles are per-theme in 0.35; write both so the palette applies
        // whichever theme the OS reports.
        ctx.all_styles_mut(|style| {
            p.apply(style);
            style.spacing.item_spacing.y = 4.0;
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
            // empty background starts a window drag.
            let bg = ui.interact(
                ui.available_rect_before_wrap(),
                egui::Id::new("bg-drag"),
                egui::Sense::click_and_drag(),
            );
            if bg.drag_started() && !ui.ctx().egui_wants_pointer_input() {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }

            self.title_bar(ui, &p);

            if let Some(msg) = self.notice.clone() {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(&msg)
                            .color(p.accent)
                            .size(self.settings.font_size * 0.85),
                    );
                    if ui.small_button("dismiss").clicked() {
                        self.notice = None;
                    }
                });
            }

            if self.show_settings {
                self.settings_sheet(ui, &p);
                return;
            }

            egui::Panel::bottom("composer")
                .frame(egui::Frame::NONE.outer_margin(egui::Margin {
                    top: 6,
                    ..Default::default()
                }))
                .show_separator_line(false)
                .show(ui, |ui| {
                    self.composer(ui, &p);
                });

            egui::ScrollArea::vertical()
                .id_salt("list")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    self.list(ui, &p);
                });
        });

        // Drafts are written back after drawing so the row closure can stay
        // an immutable borrow of the store.
        self.settle_edits(&ctx);
        self.save_if_due(&ctx);
    }
}

impl Flodo {
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
