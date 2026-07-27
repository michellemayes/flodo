#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod model;

use eframe::egui;

const WINDOW_RADIUS: u8 = 12;

fn main() -> eframe::Result<()> {
    let viewport = egui::ViewportBuilder::default()
        .with_title("Flodo")
        .with_inner_size([340.0, 480.0])
        .with_min_inner_size([260.0, 180.0])
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(true)
        .with_always_on_top();

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native("Flodo", options, Box::new(|_cc| Ok(Box::new(Flodo::new()))))
}

struct Flodo {
    store: model::Store,
}

impl Flodo {
    fn new() -> Self {
        let mut store = model::Store::default();
        store.add("Prove the screenshot loop works");
        store.add("Wire up the model and store");
        store.add("Make it beautiful");
        let first = store.todos[0].id;
        store.toggle(first);
        Self { store }
    }
}

impl eframe::App for Flodo {
    /// Transparent so our own rounded rect defines the window shape.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        let bg = egui::Color32::from_rgb(0x13, 0x10, 0x15);
        let surface = egui::Color32::from_rgb(0x1f, 0x1a, 0x22);
        let text = egui::Color32::from_rgb(0xed, 0xe9, 0xef);
        let muted = egui::Color32::from_rgb(0x8a, 0x80, 0x8f);
        let accent = egui::Color32::from_rgb(0xf7, 0x83, 0xac);

        let frame = egui::Frame::NONE
            .fill(bg)
            .corner_radius(WINDOW_RADIUS)
            .stroke(egui::Stroke::new(1.0, surface))
            .inner_margin(10.0);

        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
            // Allocate the whole background FIRST so real widgets, drawn after,
            // win the pointer and only empty space starts a window drag.
            let bg_rect = ui.available_rect_before_wrap();
            let bg_resp = ui.interact(
                bg_rect,
                egui::Id::new("bg-drag"),
                egui::Sense::click_and_drag(),
            );
            if bg_resp.drag_started() {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }

            ui.horizontal(|ui| {
                let (dot, _) = ui.allocate_exact_size(egui::Vec2::splat(8.0), egui::Sense::hover());
                ui.painter().circle_filled(dot.center(), 4.0, accent);
                ui.add_space(2.0);
                ui.label(egui::RichText::new("Flodo").color(muted).size(12.0));
            });

            ui.add_space(10.0);

            for todo in &self.store.todos {
                ui.horizontal(|ui| {
                    let (r, _) =
                        ui.allocate_exact_size(egui::Vec2::splat(16.0), egui::Sense::hover());
                    let c = r.center();
                    if todo.done {
                        ui.painter().circle_filled(c, 8.0, accent);
                    } else {
                        ui.painter()
                            .circle_stroke(c, 7.0, egui::Stroke::new(1.5, muted));
                    }
                    ui.add_space(6.0);
                    let mut rt = egui::RichText::new(&todo.title)
                        .color(if todo.done { muted } else { text })
                        .size(14.0);
                    if todo.done {
                        rt = rt.strikethrough();
                    }
                    ui.label(rt);
                });
                ui.add_space(6.0);
            }
        });
    }
}
