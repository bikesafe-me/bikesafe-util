#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use anyhow::Result;
use eframe::egui;
use std::path::Path;
use std::path::PathBuf;

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
        Box::new(|_cc| Ok(Box::new(MyApp::new()))),
    )
}

enum Message {
    Progress(usize),
    Error(String),
    Finished,
}

#[derive(Default)]
struct MyApp {
    picked_path: Option<PathBuf>,
    progress: usize,
    file_valid: Option<bool>,
    error: Option<String>,
}

impl MyApp {
    fn new() -> Self {
        Self {
            picked_path: None,
            progress: 0,
            file_valid: None,
            error: None,
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("BikeSafe Firmware Update Util");
            if self.picked_path.is_none() {
                ui.label("Select a firmware file to update your BikeSafe device.");
            }
            if let Some(error) = &self.error {
                ui.label(error).highlight();
            }

            if ui.button("Open file…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("firmware", &["bin"])
                    .pick_file()
                {
                    self.picked_path = Some(path);
                }
            }

            if let Some(path) = &self.picked_path {
                if self.file_valid.is_none() {
                    // Check if the file is valid (e.g., check the extension)
                    if path.extension().and_then(|s| s.to_str()) == Some("bin") {
                        match validate_firmware(path) {
                            Ok(_) => {
                                self.file_valid = Some(true);
                                self.error = None;
                            }
                            Err(e) => {
                                self.file_valid = Some(false);
                                self.error = Some(format!("Invalid firmware file: {}", e));
                            }
                        }
                    } else {
                        self.file_valid = Some(false);
                        self.error =
                            Some("Invalid file type. Please select a .bin file.".to_string());
                    }
                }
                let path_str = path.display().to_string();
                ui.horizontal(|ui| {
                    ui.label("Firmware Path:");
                    ui.monospace(path_str.clone());
                });

                if self.file_valid.unwrap_or(false) {
                    if ui.button("Update firmware").clicked() {
                        std::thread::spawn(move || {
                            // Simulate a long-running task
                            std::thread::sleep(std::time::Duration::from_secs(2));
                            println!("Updating firmware from {}", path_str);
                            // Here you would call the actual firmware update function
                        });
                    }
                }
            } else {
                ui.label("No firmware file selected.");
            }
        });
    }
}

fn validate_firmware(path: &Path) -> Result<()> {
    const FLASH_ORIGIN: u32 = 0x0800_4000;
    const FLASH_LEN: u32 = 48 * 1024;
    const RAM_ORIGIN: u32 = 0x2000_0000 + 0x10;
    const RAM_LEN: u32 = 20 * 1024 - 0x10;
    const KEY_STAY_IN_BOOT: u32 = 0xB0D4_2B89;

    let data = std::fs::read(path)?;
    let len = data.len() as u32;
    anyhow::ensure!(
        len <= FLASH_LEN,
        "Firmware too large: {} > {} bytes",
        len,
        FLASH_LEN
    );

    // Vector table:
    let sp = u32::from_le_bytes(data[0..4].try_into()?);
    let reset = u32::from_le_bytes(data[4..8].try_into()?);

    let ram_end = RAM_ORIGIN + RAM_LEN;
    anyhow::ensure!(
        sp >= RAM_ORIGIN && sp <= ram_end,
        "Invalid initial SP: {:#010X}, expected between {:#010X} and {:#010X}",
        sp,
        RAM_ORIGIN,
        ram_end
    );

    let flash_end = FLASH_ORIGIN + FLASH_LEN;
    anyhow::ensure!(
        reset >= FLASH_ORIGIN && reset < flash_end,
        "Invalid reset vector: {:#010X}, expected between {:#010X} and {:#010X}",
        reset,
        FLASH_ORIGIN,
        flash_end
    );

    let offset = reset - FLASH_ORIGIN;
    anyhow::ensure!(
        offset < len,
        "Reset vector at {:#X} points past end of file (offset {:#X}, len {:#X})",
        reset,
        offset,
        len
    );

    // // 2) KEY_STAY_IN_BOOT magic must appear in the blob
    // let magic_bytes = KEY_STAY_IN_BOOT.to_le_bytes();
    // anyhow::ensure!(
    //     data.windows(4).any(|w| w == magic_bytes),
    //     "Missing DFU stay-in-boot magic 0x{:08X} in firmware",
    //     KEY_STAY_IN_BOOT
    // );

    Ok(())
}
