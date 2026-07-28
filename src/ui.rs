//! Hand-drawn icons.
//!
//! Every glyph is painted with primitives rather than taken from an icon font,
//! so nothing here breaks when the user picks a font that lacks a codepoint.

use crate::theme::Palette;
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};

pub const ICON: f32 = 16.0;

fn stroke(c: Color32) -> Stroke {
    Stroke::new(1.4, c)
}

/// The completion circle. Filled with the accent and marked with a real check
/// when done; a thin ring otherwise.
///
/// `t` is how far through the check-off animation the row is, 0 to 1. The disc
/// grows out of the ring and the check is drawn over it, so ticking something
/// off reads as one motion rather than a swap between two pictures.
pub fn checkbox(painter: &egui::Painter, rect: Rect, t: f32, hovered: bool, p: &Palette) {
    let c = rect.center();
    let r = 8.0;
    let t = t.clamp(0.0, 1.0);

    if t < 1.0 {
        let ring = if hovered { p.accent } else { p.muted };
        painter.circle_stroke(c, r - 1.0, Stroke::new(1.5, ring.gamma_multiply(1.0 - t)));
    }
    if t > 0.0 {
        // A touch of overshoot near the end, so the disc lands rather than
        // simply appearing.
        let pop = 1.0 + 0.14 * (t * std::f32::consts::PI).sin();
        painter.circle_filled(c, r * t * pop, p.accent.gamma_multiply(t));
        check_glyph(painter, c, r * t, p.on_accent.gamma_multiply(t));
    }
}

/// The title-bar mark: a ring that fills clockwise as the list gets done.
/// Empty list, and it is just a dot — there is no progress to show yet.
pub fn progress_ring(painter: &egui::Painter, rect: Rect, done: usize, total: usize, p: &Palette) {
    let c = rect.center();
    let r = 5.5;
    if total == 0 {
        painter.circle_filled(c, 4.0, p.accent);
        return;
    }
    if done == total {
        painter.circle_filled(c, r, p.accent);
        check_glyph(painter, c, r, p.on_accent);
        return;
    }
    painter.circle_stroke(c, r, Stroke::new(1.6, p.muted.gamma_multiply(0.45)));
    let frac = done as f32 / total as f32;
    if frac > 0.0 {
        let steps = ((frac * 24.0).ceil() as usize).max(2);
        let arc: Vec<Pos2> = (0..=steps)
            .map(|i| {
                // Start at twelve o'clock and sweep clockwise.
                let a = -std::f32::consts::FRAC_PI_2
                    + std::f32::consts::TAU * frac * (i as f32 / steps as f32);
                c + Vec2::new(a.cos(), a.sin()) * r
            })
            .collect();
        painter.add(egui::Shape::line(arc, Stroke::new(1.8, p.accent)));
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

/// A borderless action word — "Undo", "Done". Underlined on hover so it still
/// reads as something you can click without carrying a button's weight.
pub fn text_button(ui: &mut egui::Ui, p: &Palette, text: &str, size: f32) -> egui::Response {
    let resp = ui.add(
        egui::Label::new(egui::RichText::new(text).color(p.accent).size(size))
            .sense(egui::Sense::click())
            .selectable(false),
    );
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        let r = resp.rect;
        ui.painter()
            .hline(r.x_range(), r.bottom() - 1.0, Stroke::new(1.0, p.accent));
    }
    resp
}

/// A hairline divider. Thinner and quieter than `ui.separator()`, which draws
/// at widget weight and cuts the settings sheet into slabs.
pub fn rule(ui: &mut egui::Ui, p: &Palette) {
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        Stroke::new(1.0, p.border.gamma_multiply(0.8)),
    );
}

/// A square icon button that paints via `draw` and tints itself on hover.
pub fn icon_button(
    ui: &mut egui::Ui,
    p: &Palette,
    tooltip: &str,
    draw: impl FnOnce(&egui::Painter, Rect, Color32),
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(ICON + 6.0), egui::Sense::click());
    let hovered = resp.hovered();
    if hovered {
        ui.painter()
            .rect_filled(rect.shrink(1.0), 5.0, p.surface_hover);
    }
    let color = if hovered { p.text } else { p.muted };
    draw(ui.painter(), rect, color);
    resp.on_hover_text(tooltip)
}
