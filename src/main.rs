// Windows release builds are GUI subsystem, so they have no console attached.
// The CLI subcommands therefore print nothing there; use a debug build or a
// terminal that redirects output. macOS and Linux are unaffected.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod cli;
mod fonts;
mod hotkey;
mod markdown;
mod model;
mod settings;
mod store;
mod theme;
mod ui;

use eframe::egui;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let command = match cli::parse(&args) {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("flodo: {msg}\n");
            eprint!("{}", cli::USAGE);
            return ExitCode::FAILURE;
        }
    };

    if command == cli::Command::Gui {
        return match run_gui() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("flodo: {e}");
                ExitCode::FAILURE
            }
        };
    }

    cli::run(command)
}

fn run_gui() -> eframe::Result<()> {
    let (settings, _) = store::load_settings();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Flodo")
        .with_app_id("flodo")
        .with_inner_size([settings.window.w, settings.window.h])
        .with_min_inner_size([260.0, 180.0])
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(true)
        .with_clamp_size_to_monitor_size(true);

    if settings.always_on_top {
        viewport = viewport.with_always_on_top();
    }
    if let Some((x, y)) = settings.window.position() {
        viewport = viewport.with_position([x, y]);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Flodo",
        options,
        Box::new(|cc| Ok(Box::new(app::Flodo::new(cc)))),
    )
}
