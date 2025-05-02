#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use std::path::PathBuf;

use eframe::egui;

fn main() -> eframe::Result {
    env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([640.0, 240.0]) // wide enough for the drag-drop overlay text
            .with_resizable(false)
            .with_drag_and_drop(true),
        persist_window: true,
        ..Default::default()
    };
    eframe::run_native(
        "BikeSafe Firmware Update Util",
        options,
        Box::new(|_cc| Ok(Box::<MyApp>::default())),
    )
}

#[derive(Default)]
struct MyApp {
    picked_path: Option<PathBuf>,
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("BikeSafe Firmware Update Util");
            ui.label("Select a firmware file to update your BikeSafe device.");

            if ui.button("Open file…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("firmware", &["bin"])
                    .pick_file()
                {
                    self.picked_path = Some(path);
                }
            }

            if let Some(path) = &self.picked_path {
                let path_str = path.display().to_string();
                ui.horizontal(|ui| {
                    ui.label("Firmware Path:");
                    ui.monospace(path_str.clone());
                });

                if ui.button("Update firmware").clicked() {
                    std::thread::spawn(move || {
                        // Simulate a long-running task
                        std::thread::sleep(std::time::Duration::from_secs(2));
                        println!("Updating firmware from {}", path_str);
                        // Here you would call the actual firmware update function
                    });
                }
            } else {
                ui.label("No firmware file selected.");
            }
        });
    }
}
