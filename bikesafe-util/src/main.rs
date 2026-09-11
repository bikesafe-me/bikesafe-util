#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::fs::File;
use std::io::{self, Seek};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context as _, Result};
use dfu_libusb::*;
use iced::futures::channel::oneshot;
use iced::widget::{button, column, container, progress_bar, row, text};
use iced::{Element, Fill, Length, Subscription, Task, Theme};

fn main() -> iced::Result {
    env_logger::init();
    iced::application(App::default, App::update, App::view)
        .title(|_: &App| String::from("BrakeBright Firmware Update Util"))
        .theme(|_: &App| Theme::Dark)
        .subscription(App::subscription)
        .window_size((640.0, 320.0))
        .resizable(false)
        .centered()
        .run()
}

// ── State ────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct App {
    picked_path: Option<PathBuf>,
    file_valid: Option<bool>,
    file_error: Option<String>,
    device_connected: bool,
    flashing: bool,
    progress: f32,
    flash_done: bool,
    flash_error: Option<String>,
    receiver: Option<mpsc::Receiver<FlashEvent>>,
}

enum FlashEvent {
    Progress(f32),
    Done,
    Error(String),
}

// ── Messages ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Message {
    OpenFilePicker,
    FilePicked(Option<PathBuf>),
    StartFlash,
    Tick,
}

// ── Update ───────────────────────────────────────────────────────────────────

impl App {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::OpenFilePicker => Task::perform(
                async {
                    let (tx, rx) = oneshot::channel();
                    std::thread::spawn(move || {
                        let path = rfd::FileDialog::new()
                            .add_filter("DFU firmware", &["bin"])
                            .pick_file();
                        let _ = tx.send(path);
                    });
                    rx.await.unwrap_or(None)
                },
                Message::FilePicked,
            ),

            Message::FilePicked(maybe_path) => {
                if let Some(path) = maybe_path {
                    self.flash_done = false;
                    self.flash_error = None;
                    self.progress = 0.0;
                    self.receiver = None;
                    match validate_firmware(&path) {
                        Ok(_) => {
                            self.file_valid = Some(true);
                            self.file_error = None;
                        }
                        Err(e) => {
                            self.file_valid = Some(false);
                            self.file_error = Some(e.to_string());
                        }
                    }
                    self.picked_path = Some(path);
                }
                Task::none()
            }

            Message::StartFlash => {
                let Some(path) = self.picked_path.clone() else {
                    return Task::none();
                };
                self.flashing = true;
                self.progress = 0.0;
                self.flash_done = false;
                self.flash_error = None;

                let (tx, rx) = mpsc::channel();
                self.receiver = Some(rx);
                std::thread::spawn(move || {
                    let result = do_flash(&path, tx.clone());
                    if let Err(e) = result {
                        let _ = tx.send(FlashEvent::Error(e.to_string()));
                    }
                });
                Task::none()
            }

            Message::Tick => {
                if self.flashing {
                    // Drain flash progress channel
                    if let Some(rx) = &self.receiver {
                        let mut done = false;
                        for event in rx.try_iter() {
                            match event {
                                FlashEvent::Progress(p) => {
                                    self.progress = (self.progress + p).min(1.0);
                                }
                                FlashEvent::Done => {
                                    self.flashing = false;
                                    self.flash_done = true;
                                    self.progress = 1.0;
                                    done = true;
                                }
                                FlashEvent::Error(e) => {
                                    self.flashing = false;
                                    self.flash_error = Some(e);
                                    done = true;
                                }
                            }
                            if done {
                                break;
                            }
                        }
                        if done {
                            self.receiver = None;
                        }
                    }
                } else if self.file_valid == Some(true) {
                    // Poll for device connection
                    self.device_connected = check_device_connected();
                }
                Task::none()
            }
        }
    }

    // ── View ──────────────────────────────────────────────────────────────────

    fn view(&self) -> Element<'_, Message> {
        let title = text("BrakeBright Firmware Update Util")
            .size(22)
            .style(text::primary);

        let file_info: Element<'_, Message> = match &self.picked_path {
            Some(path) => row![
                text("Firmware:").style(text::secondary),
                text(path.display().to_string()),
            ]
            .spacing(6)
            .into(),
            None => text("Select a .bin firmware file to flash your device.")
                .style(text::secondary)
                .into(),
        };

        let open_btn = button("  Open file…  ").on_press(Message::OpenFilePicker);

        let status: Element<'_, Message> = if let Some(err) = &self.file_error {
            text(format!("✗  {err}")).style(text::danger).into()
        } else if self.flash_done {
            text("✓  Flash complete! Test the device by tilting it.")
                .size(15)
                .style(text::success)
                .into()
        } else if let Some(err) = &self.flash_error {
            text(format!("✗  Flash error: {err}"))
                .style(text::danger)
                .into()
        } else if self.flashing {
            column![
                container(progress_bar(0.0..=1.0, self.progress)).height(Length::Fixed(20.0)),
                text(format!("Flashing… {:.0}%", self.progress * 100.0)).style(text::secondary),
            ]
            .spacing(6)
            .into()
        } else if self.file_valid == Some(true) {
            if self.device_connected {
                button("  ⚡ Flash Firmware  ")
                    .on_press(Message::StartFlash)
                    .style(button::success)
                    .into()
            } else {
                text("Connect the device in DFU mode (LED blinks rapidly) then wait…")
                    .style(text::secondary)
                    .into()
            }
        } else {
            text("").into()
        };

        container(
            column![title, file_info, open_btn, status]
                .spacing(16)
                .padding(28)
                .width(Fill),
        )
        .width(Fill)
        .into()
    }

    // ── Subscription ──────────────────────────────────────────────────────────

    fn subscription(&self) -> Subscription<Message> {
        if self.flashing {
            // Poll at 50 ms for smooth progress bar updates
            iced::time::every(Duration::from_millis(50)).map(|_| Message::Tick)
        } else if self.file_valid == Some(true) && !self.flash_done {
            // Poll for device presence every 200 ms
            iced::time::every(Duration::from_millis(200)).map(|_| Message::Tick)
        } else {
            Subscription::none()
        }
    }
}

// ── DFU flash ────────────────────────────────────────────────────────────────

fn check_device_connected() -> bool {
    rusb::Context::new()
        .ok()
        .and_then(|ctx| DfuLibusb::open(&ctx, 0x1209, 0x2444, 0, 0).ok())
        .is_some()
}

fn do_flash(path: &Path, tx: mpsc::Sender<FlashEvent>) -> Result<()> {
    let context = rusb::Context::new()?;
    let mut device =
        DfuLibusb::open(&context, 0x1209, 0x2444, 0, 0).context("could not open device")?;

    let mut file =
        File::open(path).with_context(|| format!("could not open `{}`", path.display()))?;
    let file_size =
        u32::try_from(file.seek(io::SeekFrom::End(0))?).context("firmware file too large")?;
    file.seek(io::SeekFrom::Start(0))?;

    device.with_progress({
        let tx = tx.clone();
        move |count| {
            let frac = count as f32 / file_size as f32;
            let _ = tx.send(FlashEvent::Progress(frac));
        }
    });

    device.override_address(0x08004000);

    match device.download(file, file_size) {
        Ok(Some(_device)) => {
            let _ = tx.send(FlashEvent::Done);
        }
        Ok(None) => {
            // The device left DFU by resetting itself after manifestation.
            let _ = tx.send(FlashEvent::Done);
        }
        Err(Error::LibUsb(e)) => {
            // Device likely reset itself after a successful flash
            log::warn!("USB error after download (device reset itself): {e:?}");
            let _ = tx.send(FlashEvent::Done);
        }
        Err(e) => {
            return Err(anyhow::anyhow!("download failed: {e:?}"));
        }
    }

    Ok(())
}

// ── Firmware validation ───────────────────────────────────────────────────────

fn validate_firmware(path: &Path) -> Result<()> {
    const FLASH_ORIGIN: u32 = 0x0800_4000;
    const FLASH_LEN: u32 = 48 * 1024;
    const RAM_ORIGIN: u32 = 0x2000_0000 + 0x10;
    const RAM_LEN: u32 = 20 * 1024 - 0x10;

    let data = std::fs::read(path)?;
    let len = data.len() as u32;
    anyhow::ensure!(len >= 8, "firmware file is too small: {len} bytes");
    anyhow::ensure!(
        len <= FLASH_LEN,
        "firmware too large: {} > {} bytes",
        len,
        FLASH_LEN
    );

    let sp = u32::from_le_bytes(data[0..4].try_into()?);
    let reset = u32::from_le_bytes(data[4..8].try_into()?);

    let ram_end = RAM_ORIGIN + RAM_LEN;
    anyhow::ensure!(
        sp >= RAM_ORIGIN && sp <= ram_end,
        "invalid initial SP {sp:#010X} (expected {RAM_ORIGIN:#010X}–{ram_end:#010X})",
    );

    let flash_end = FLASH_ORIGIN + FLASH_LEN;
    anyhow::ensure!(
        reset >= FLASH_ORIGIN && reset < flash_end,
        "invalid reset vector {reset:#010X} (expected {FLASH_ORIGIN:#010X}–{flash_end:#010X})",
    );

    let offset = reset - FLASH_ORIGIN;
    anyhow::ensure!(
        offset < len,
        "reset vector {reset:#010X} points past end of file (offset {offset:#X}, len {len:#X})",
    );

    Ok(())
}
