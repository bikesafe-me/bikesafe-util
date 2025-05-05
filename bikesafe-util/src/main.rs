#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use std::fs::File;
use std::io::{self, Seek};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use dfu_libusb::*;
use eframe::egui::{self, ProgressBar};

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
        "BrakeBright Firmware Update Util",
        options,
        Box::new(|_cc| Ok(Box::new(MyApp::new()))),
    )
}

const PROGRESS_INIT: f32 = 0.000001; // avoid 0% progress bar

#[derive(Default)]
struct MyApp {
    picked_path: Option<PathBuf>,
    progress: f32,
    receiver: Option<Receiver<f32>>,
    file_valid: Option<bool>,
    error: Option<String>,
}

impl MyApp {
    fn new() -> Self {
        Self {
            picked_path: None,
            progress: PROGRESS_INIT,
            file_valid: None,
            error: None,
            receiver: None,
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("BrakeBright Firmware Update Util");

            if let Some(path) = &self.picked_path {
                let path_str = path.display().to_string();
                ui.horizontal(|ui| {
                    ui.label("Firmware Path:");
                    ui.monospace(path_str);
                });
            } else {
                ui.label("Select a firmware file to update your BrakeBright device.");
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
                    self.file_valid = None;
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

                if self.file_valid.unwrap_or(false) {
                    ui.label("_____________________________________________________");
                    // CLI logic adapted
                    let vid = 0x1209;
                    let pid = 0x2444;
                    let intf = 0;
                    let alt = 0;
                    let context = rusb::Context::new().expect("Failed to create USB context");
                    if DfuLibusb::open(&context, 0x1209, 0x2444, 0, 0).is_ok()
                    {
                        if ui.button("Update firmware").clicked() {
                            ui.label("Updating firmware...");
                            let (tx, rx) = mpsc::channel();
                            self.receiver = Some(rx);

                            let path = path.clone();
                            thread::spawn(move || {
                                let mut device = DfuLibusb::open(&context, vid, pid, intf, alt)
                                    .context("could not open device")
                                    .unwrap();

                                let mut file = File::open(&path)
                                    .with_context(|| {
                                        format!("could not open firmware file `{}`", path.display())
                                    })
                                    .unwrap();
                                let file_size =
                                    u32::try_from(file.seek(io::SeekFrom::End(0)).unwrap())
                                        .context("The firmware file is too big")
                                        .unwrap();
                                file.seek(io::SeekFrom::Start(0)).unwrap();

                                // Progress via DFU core
                                device.with_progress({
                                    let tx = tx.clone();
                                    move |count| {
                                        // count is bytes since last callback
                                        let prog = count as f32 / file_size as f32;
                                        let _ = tx.send(prog);
                                    }
                                });

                                // Optionally override start address
                                device.override_address(0x08004000);

                                // Perform download
                                match device.download(file, file_size) {
                                    Ok(_) => (),
                                    Err(e) => log::error!("Download error: {e:?}"),
                                };
                            });
                        }
                    } else if self.receiver.is_none() {
                        ui.label(
                            "Please make sure the USB is connected and the device is in DFU mode. (LED blinking constantly)",
                        );
                        ctx.request_repaint_after(Duration::from_millis(100));
                    }

                    if let Some(rx) = &self.receiver {
                        for p in rx.try_iter() {
                            self.progress += p;
                        }
                        log::error!("Progress: {}", self.progress);
                        ui.add(ProgressBar::new(self.progress).show_percentage());
                        if self.progress >= 1.0 {
                            ui.label("Flash complete! Please test the device function by tilting it.");
                        } else {
                            ctx.request_repaint();
                        }
                    }
                } else {
                    ui.label("Please select a valid firmware file.");
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
