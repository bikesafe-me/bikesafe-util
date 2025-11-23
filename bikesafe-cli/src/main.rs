use std::io::{self, Seek};
use std::path::PathBuf;

use anyhow::{Context, Result};
use dfu_core::DfuIo; /* Import the Dfu trait to bring
 * functional_descriptor into scope */
use dfu_libusb::*;

#[derive(clap::Parser)]
pub struct Cli {
    /// Path to the firmware file to write to the device.
    #[clap(long, short)]
    path: Option<PathBuf>,

    /// Specify Vendor/Product ID(s) of DFU device.
    /// i.e. 1209:2444
    #[clap(
        long,
        short,
        value_parser = Self::parse_vid_pid, name = "VID>:<PID",
        default_value = "0x1209:0x2444"
    )]
    device: (u16, u16),

    /// target address to flash the firmware
    #[clap(long, short, default_value = "0x08004000", value_parser = Self::parse_address)]
    address: Option<u32>,

    /// Specify the DFU Interface number.
    #[clap(long, short, default_value = "0")]
    intf: u8,

    /// Specify the Altsetting of the DFU Interface by number.
    #[clap(long, default_value = "0")]
    alt: u8,

    /// Reset after download.
    #[clap(short, long)]
    reset: bool,

    /// Enable verbose logs.
    #[clap(long, short)]
    verbose: bool,

    #[clap(long)]
    /// print info and exit
    info: bool,
}

impl Cli {
    pub fn run(self) -> Result<()> {
        let Cli {
            device,
            intf,
            alt,
            verbose,
            path,
            reset,
            info,
            address,
        } = self;
        let log_level = if verbose {
            simplelog::LevelFilter::Trace
        } else {
            simplelog::LevelFilter::Info
        };
        simplelog::SimpleLogger::init(log_level, Default::default())?;
        let (vid, pid) = device;
        let context = rusb::Context::new()?;

        let device: Dfu<rusb::Context> =
            DfuLibusb::open(&context, vid, pid, intf, alt).context("could not open device")?;

        println!("{:?}", device.into_inner().functional_descriptor());
        if info {
            return Ok(());
        }
        let mut device: Dfu<rusb::Context> =
            DfuLibusb::open(&context, vid, pid, intf, alt).context("could not open device")?;

        if let Some(path) = path {
            let mut file = std::fs::File::open(&path)
                .with_context(|| format!("could not open firmware file `{}`", path.display()))?;
            let file_size = u32::try_from(file.seek(io::SeekFrom::End(0))?)
                .context("The firmware file is too big")?;
            file.seek(io::SeekFrom::Start(0))?;

            let bar = indicatif::ProgressBar::new(file_size as u64);
            bar.set_style(
                indicatif::ProgressStyle::default_bar()
                    .template(
                        "{spinner:.green} [{elapsed_precise}] [{bar:27.cyan/blue}] \
                    {bytes}/{total_bytes} ({bytes_per_sec}) ({eta}) {msg:10}",
                    )?
                    .progress_chars("#>-"),
            );

            device.with_progress({
                let bar = bar.clone();
                move |count| {
                    bar.inc(count as u64);
                    if bar.position() == file_size as u64 {
                        bar.finish();
                    }
                }
            });

            if let Some(address) = address {
                device.override_address(address);
            }

            match device.download(file, file_size) {
                Ok(_) => (),
                Err(Error::LibUsb(e)) => {
                    if bar.is_finished() {
                        // Some devices reset themselves after a successful
                        // download, causing a LIBUSB_ERROR_NO_DEVICE error
                        // when we try to communicate further.
                        eprintln!("{e:#?}");
                        println!("Download successful; Device reseted itself");
                        return Ok(());
                    } else {
                        eprintln!("Firmware download failed: {e:#?}");
                    }
                    return Ok(());
                }
                e => {
                    return e.context("could not write firmware to the device");
                }
            }
        }

        if reset {
            // Detach isn't strictly meant to be sent after a download, however
            // u-boot in particular will only switch to the
            // downloaded firmware if it saw a detach before
            // a usb reset. So send a detach blindly...
            //
            // This matches the behaviour of dfu-util so should be safe
            if device.will_detach() {
                println!("Detaching device");
                device.detach()?;
            } else {
                println!("Device does not support detach");
            }

            println!("Resetting device");
            device.usb_reset()?;
        }

        Ok(())
    }

    pub fn parse_vid_pid(s: &str) -> Result<(u16, u16)> {
        let (vid, pid) = s
            .split_once(':')
            .context("could not parse VID/PID (missing `:')")?;
        // remove leading 0x if present
        let vid = vid.strip_prefix("0x").unwrap_or(vid);
        let pid = pid.strip_prefix("0x").unwrap_or(pid);
        if vid.len() != 4 || pid.len() != 4 {
            return Err(anyhow::anyhow!("VID/PID must be 4 digits each"));
        }
        let vid = u16::from_str_radix(vid, 16).context("could not parse VID")?;
        let pid = u16::from_str_radix(pid, 16).context("could not parse PID")?;

        Ok((vid, pid))
    }

    pub fn parse_address(s: &str) -> Result<u32> {
        // remove leading 0x if present
        let s = s.strip_prefix("0x").unwrap_or(s);
        let address = u32::from_str_radix(s, 16).context("could not parse address")?;
        Ok(address)
    }
}

fn main() -> Result<()> {
    <Cli as clap::Parser>::parse().run()
}
