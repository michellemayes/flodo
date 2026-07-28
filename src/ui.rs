//! Hand-drawn icons.
//!
//! Every glyph is painted with primitives rather than taken from an icon font,
//! so nothing here breaks when the user picks a font that lacks a codepoint.

use crate::theme::Palette;
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};

pub const ICON: f32 = 16.0;
/// Side of the square an [`icon_button`] occupies: the glyph plus its padding.
pub const BUTTON: f32 = ICON + 6.0;

fn stroke(c: Color32) -> Stroke {
    Stroke::new(1.4, c)
}

/// The completion circle. Filled with the accent and marked with a real check
/// when done; a thin ring otherwise.
pub fn checkbox(painter: &egui::Painter, rect: Rect, done: bool, hovered: bool, p: &Palette) {
    let c = rect.center();
    let r = 8.0;
    if done {
        painter.circle_filled(c, r, p.accent);
        check_glyph(painter, c, r, p.on_accent);
    } else {
        let ring = if hovered { p.accent } else { p.muted };
        painter.circle_stroke(c, r - 1.0, Stroke::new(1.5, ring));
    }
}

/// A checkmark polyline sized relative to the disc it sits on.
fn check_glyph(painter: &egui::Painter, c: Pos2, r: f32, color: Color32) {
    let s = r / 8.0;
    let pts = vec![
        c + Vec2::new(-3.6 * s, 0.2 * s),
        c + Vec2::new(-1.2 * s, 2.8 * s),
        c + Vec2::new(3.8 * s, -2.8 * s),
    ];
    painter.add(egui::Shape::line(
        pts,
        Stroke::new((2.0 * s).max(1.4), color),
    ));
}

pub fn chevron(painter: &egui::Painter, rect: Rect, expanded: bool, color: Color32) {
    let c = rect.center();
    let s = 3.2;
    let pts = if expanded {
        // Pointing down.
        vec![
            c + Vec2::new(-s, -s * 0.6),
            c + Vec2::new(0.0, s * 0.7),
            c + Vec2::new(s, -s * 0.6),
        ]
    } else {
        // Pointing right.
        vec![
            c + Vec2::new(-s * 0.6, -s),
            c + Vec2::new(s * 0.7, 0.0),
            c + Vec2::new(-s * 0.6, s),
        ]
    };
    painter.add(egui::Shape::line(pts, stroke(color)));
}

pub fn close(painter: &egui::Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let s = 4.0;
    painter.line_segment([c + Vec2::new(-s, -s), c + Vec2::new(s, s)], stroke(color));
    painter.line_segment([c + Vec2::new(s, -s), c + Vec2::new(-s, s)], stroke(color));
}

/// An eye, open when completed todos are visible and struck through when not.
pub fn eye(painter: &egui::Painter, rect: Rect, open: bool, color: Color32) {
    let c = rect.center();
    let (w, h) = (6.5, 3.6);
    // Two arcs approximated with polylines make a lens shape.
    let arc = |up: bool| {
        let sign = if up { -1.0 } else { 1.0 };
        (0..=10)
            .map(|i| {
                let t = i as f32 / 10.0;
                let x = -w + 2.0 * w * t;
                let y = sign * h * (1.0 - (x / w).powi(2)).max(0.0).sqrt();
                c + Vec2::new(x, y)
            })
            .collect::<Vec<_>>()
    };
    painter.add(egui::Shape::line(arc(true), stroke(color)));
    painter.add(egui::Shape::line(arc(false), stroke(color)));
    painter.circle_filled(c, 1.7, color);
    if !open {
        painter.line_segment(
            [c + Vec2::new(-w, h * 1.4), c + Vec2::new(w, -h * 1.4)],
            stroke(color),
        );
    }
}

/// A pushpin. Filled head when pinned (always-on-top), outline when not.
pub fn pin(painter: &egui::Painter, rect: Rect, pinned: bool, color: Color32) {
    let c = rect.center();
    let head = c + Vec2::new(0.0, -1.5);
    if pinned {
        painter.circle_filled(head, 3.6, color);
    } else {
        painter.circle_stroke(head, 3.2, stroke(color));
    }
    painter.line_segment(
        [head + Vec2::new(0.0, 3.2), c + Vec2::new(0.0, 6.5)],
        stroke(color),
    );
}

/// Three sliders. Chosen over a gear because it is legible at 16px when drawn
/// with primitives, which a gear is not.
pub fn sliders(painter: &egui::Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let w = 5.5;
    for (i, knob) in [(-4.0, -1.6), (0.0, 2.0), (4.0, -0.6)]
        .into_iter()
        .enumerate()
    {
        let y = c.y + knob.0;
        painter.line_segment(
            [Pos2::new(c.x - w, y), Pos2::new(c.x + w, y)],
            Stroke::new(1.3, color),
        );
        let _ = i;
        painter.circle_filled(Pos2::new(c.x + knob.1, y), 1.9, color);
    }
}

pub fn plus(painter: &egui::Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let s = 4.0;
    painter.line_segment(
        [c + Vec2::new(-s, 0.0), c + Vec2::new(s, 0.0)],
        stroke(color),
    );
    painter.line_segment(
        [c + Vec2::new(0.0, -s), c + Vec2::new(0.0, s)],
        stroke(color),
    );
}

/// The drag handle: two columns of dots, shown only on hover.
pub fn grip(painter: &egui::Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    for dy in [-3.5f32, 0.0, 3.5] {
        for dx in [-1.8f32, 1.8] {
            painter.circle_filled(c + Vec2::new(dx, dy), 1.1, color);
        }
    }
}

/// A square icon button that paints via `draw` and tints itself on hover.
pub fn icon_button(
    ui: &mut egui::Ui,
    p: &Palette,
    tooltip: &str,
    draw: impl FnOnce(&egui::Painter, Rect, Color32),
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(BUTTON), egui::Sense::click());
    paint_icon_button(ui, p, rect, resp, tooltip, draw)
}

/// An [`icon_button`] that is only drawn when `shown`, but always occupies its
/// slot, and answers to a caller-chosen id.
///
/// Both matter for the buttons a row reveals on hover. If the slot collapsed
/// while hidden, every icon beside it would slide sideways the moment the
/// pointer entered the row — landing this button exactly where the user was
/// aiming. And egui numbers automatic widget ids by order of allocation, so a
/// button that comes and goes renumbers its neighbours; a press and its
/// release then land on two different ids and the click is dropped.
pub fn hover_icon_button(
    ui: &mut egui::Ui,
    p: &Palette,
    id: egui::Id,
    shown: bool,
    tooltip: &str,
    draw: impl FnOnce(&egui::Painter, Rect, Color32),
) -> egui::Response {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(BUTTON), egui::Sense::hover());
    let sense = if shown {
        egui::Sense::click()
    } else {
        // Nothing to click while invisible, and nothing to hover either: the
        // tooltip would give away a button that is not there.
        egui::Sense::empty()
    };
    let resp = ui.interact(rect, id, sense);
    if !shown {
        return resp;
    }
    paint_icon_button(ui, p, rect, resp, tooltip, draw)
}

fn paint_icon_button(
    ui: &mut egui::Ui,
    p: &Palette,
    rect: Rect,
    resp: egui::Response,
    tooltip: &str,
    draw: impl FnOnce(&egui::Painter, Rect, Color32),
) -> egui::Response {
    // `contains_pointer` rather than `hovered`: egui reports only the pressed
    // widget as hovered while a button is held, which would drop the tint
    // half-way through a click.
    let lit = resp.contains_pointer();
    if lit {
        ui.painter()
            .rect_filled(rect.shrink(1.0), 5.0, p.surface_hover);
    }
    let color = if lit { p.text } else { p.muted };
    draw(ui.painter(), rect, color);
    resp.on_hover_text(tooltip)
}

/// The line through a completed todo.
///
/// egui draws this itself given `TextFormat::strikethrough`, but it puts the
/// line half-way down each glyph's line box and measures from the font's
/// ascent — and the ascent is rounded to whole pixels while the line box here
/// is the 1.45x leading the markdown formats ask for. The two do not grow
/// together, so the line wanders off the middle of the letters as the font
/// size changes. Hanging it off the baseline at a fixed fraction of the font
/// size keeps it centred at every size.
pub fn strike(
    painter: &egui::Painter,
    galley: &egui::Galley,
    origin: Pos2,
    color: Color32,
    size: f32,
) {
    // Roughly half the x-height of a typical UI face, which puts the line
    // through the middle of the lowercase letters and across the middle of
    // the capitals.
    let rise = size * 0.29;
    let width = (size / 13.0).max(1.0);

    for placed in &galley.rows {
        let mut glyphs = placed.row.glyphs.iter().filter(|g| !g.chr.is_whitespace());
        let Some(first) = glyphs.next() else {
            continue;
        };
        let last = glyphs.next_back().unwrap_or(first);
        let row = origin + placed.pos.to_vec2();
        // One straight line per row, taken from the first glyph's baseline:
        // a title can mix sizes (inline code is a point smaller) and a line
        // that stepped up and down mid-word would read as a mistake.
        let y = row.y + first.pos.y - rise;
        painter.line_segment(
            [
                Pos2::new(row.x + first.pos.x, y),
                Pos2::new(row.x + last.max_x(), y),
            ],
            Stroke::new(width, color),
        );
    }
}
