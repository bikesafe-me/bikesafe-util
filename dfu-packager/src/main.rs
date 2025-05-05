use std::ffi::OsStr;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use byteorder::{LittleEndian, WriteBytesExt};
use crc32fast::Hasher;

/// One contiguous image to flash at `address`.
pub struct DfuElement {
    pub address: u32,
    pub data: Vec<u8>,
}

/// A DFU “Target” (alternate interface), with a 255-byte name (padded).
pub struct DfuTarget {
    pub name: String,
    pub alternate_setting: u8,
    pub elements: Vec<DfuElement>,
}

/// Represents the entire DFU file to build.
pub struct DfuFile {
    pub device_vid: u16,
    pub device_pid: u16,
    pub targets: Vec<DfuTarget>,
}

impl DfuFile {
    /// Create and write a `.dfu` file to `out_path`.
    pub fn write_to(&self, out_path: impl AsRef<Path>) -> Result<()> {
        // 1) Build the in-memory DFU body (all Target sections).
        let mut body = Vec::new();
        for target in &self.targets {
            let mut elements_data = Vec::new();
            for element in &target.elements {
                // Element header: address + size (Little-Endian)
                elements_data.write_u32::<LittleEndian>(element.address)?;
                elements_data
                    .write_u32::<LittleEndian>(element.data.len() as u32)?;
                elements_data.extend(&element.data);
            }
            // Pad the target name to exactly 255 bytes
            let mut name_bytes = target.name.as_bytes().to_vec();
            name_bytes.resize(255, 0);

            // Target prefix (per dfuse-pack.py):
            // "Target" (6B), bAlternate (1B), dwNamed (4B), szTargetName
            // (255B), dwTargetSize (4B), dwNbElements (4B)
            body.extend(b"Target");
            body.write_u8(target.alternate_setting)?; // bAlternate
            body.write_u32::<LittleEndian>(1)?; // dwNamed = 1 (name present)
            body.extend(&name_bytes); // szTargetName (255 bytes)
            body.write_u32::<LittleEndian>(elements_data.len() as u32)?; // dwTargetSize
            body.write_u32::<LittleEndian>(target.elements.len() as u32)?; // dwNbElements

            // Append element data blocks
            body.extend(elements_data);
        }

        // 2) DFU prefix header:
        // "DfuSe" (5B), bVersion (1B), dwSize (4B), bTargets (1B)
        let mut dfu = Vec::new();
        dfu.extend(b"DfuSe");
        dfu.write_u8(1)?; // bVersion
        // dwSize = size of bTargets + body
        dfu.write_u32::<LittleEndian>((1 + body.len()) as u32)?;
        dfu.write_u8(self.targets.len() as u8)?; // bTargets
        dfu.extend(&body);

        // 3) DFU suffix (Little-Endian): bcdDevice, idProduct, idVendor,
        //    bcdDFU, "UFD", length
        dfu.write_u16::<LittleEndian>(0)?; // bcdDevice
        dfu.write_u16::<LittleEndian>(self.device_pid)?; // idProduct
        dfu.write_u16::<LittleEndian>(self.device_vid)?; // idVendor
        dfu.write_u16::<LittleEndian>(0x011A)?; // bcdDFU
        dfu.extend(b"UFD"); // signature
        dfu.write_u8(16)?; // suffix length

        // 4) CRC32 (bit-inverted)
        let mut hasher = Hasher::new();
        hasher.update(&dfu);
        let crc = !hasher.finalize();
        dfu.write_u32::<LittleEndian>(crc)?;

        // 5) Write to disk
        let mut f = File::create(out_path)?;
        f.write_all(&dfu)?;
        Ok(())
    }
}

#[derive(clap::Parser)]
pub struct Cli {
    /// Path to the firmware bin file.
    #[clap(long, short)]
    file: PathBuf,

    /// output file name
    #[clap(long, short)]
    output: Option<PathBuf>,

    /// Specify Vendor/Product ID(s) of DFU device.
    /// i.e. 1209:2444
    #[clap(
        long,
        short,
        value_parser = Self::parse_vid_pid, name = "VID>:<PID",
    )]
    device: (u16, u16),

    /// Enable verbose logs.
    #[clap(long, short)]
    verbose: bool,

    /// target address to flash the firmware
    #[clap(long, short, default_value = "08004000", value_parser = Self::parse_address)]
    address: u32,
}

impl Cli {
    pub fn run(self) -> Result<()> {
        let Cli {
            device,
            output,
            verbose,
            file,
            address,
        } = self;
        let log_level = if verbose {
            simplelog::LevelFilter::Trace
        } else {
            simplelog::LevelFilter::Info
        };
        simplelog::SimpleLogger::init(log_level, Default::default())?;
        let (vid, pid) = device;
        let mut out_path = output.unwrap_or_else(|| {
            let mut path = file.clone();
            path.set_extension("dfu");
            path
        });

        if out_path.extension() != Some(OsStr::new("dfu")) {
            eprintln!("Changing the output file to have .dfu extension");
            out_path.set_extension("dfu");
        }

        let dfu_file = DfuFile {
            device_vid: vid,
            device_pid: pid,
            targets: vec![DfuTarget {
                name: "Flash".to_string(),
                alternate_setting: 0,
                elements: vec![DfuElement {
                    address,
                    data: std::fs::read(file)
                        .context("Cannot read bin file")?,
                }],
            }],
        };

        dfu_file.write_to(out_path)?;

        Ok(())
    }

    pub fn parse_vid_pid(s: &str) -> Result<(u16, u16)> {
        let (vid, pid) = s
            .split_once(':')
            .context("could not parse VID/PID (missing `:')")?;
        let vid =
            u16::from_str_radix(vid, 16).context("could not parse VID")?;
        let pid =
            u16::from_str_radix(pid, 16).context("could not parse PID")?;

        Ok((vid, pid))
    }

    pub fn parse_address(s: &str) -> Result<u32> {
        let address =
            u32::from_str_radix(s, 16).context("could not parse address")?;
        Ok(address)
    }
}

fn main() -> Result<()> {
    <Cli as clap::Parser>::parse().run()
}
