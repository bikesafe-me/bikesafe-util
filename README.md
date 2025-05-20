# bikesafe-util

## Utility software for [BrakeBright](https://shop.bikesafe.me)

A cross-platform firmware flashing utility for the [BrakeBright](https://shop.bikesafe.me) DFU bootloader. Written in Rust with a GUI frontend (egui) and a CLI backend, this tool allows you to safely verify and download firmware images to your device via the USB DFU protocol.

## Features

- **Cross-platform**: Windows & Linux support via `rusb` + `WinUSB/libusb`
- **GUI & CLI**: egui-based desktop app plus a command-line interface
- **Firmware validation**: file-size, vector-table, and embedded magic-key checks
- **Progress reporting**: real-time progress bar, both in terminal and GUI

## Installation

### Pre-requisites

- **Linux**: `libusb` (usually pre-installed)
- **Windows**: [Zadig](https://zadig.akeo.ie/) to install the WinUSB driver
- **macOS**: Not tested!

### Entering Direct Firmware Update (DFU) Mode

- Disconnect the device from the motorcycle, at least **temporarily disconnect red and blue wires**.
- **USB-C**: Use a USB-C cable to connect the device to your PC.
- **White/Yellow LED**: The white/yellow LED should be on, indicating power. If it’s not, check the USB cable and connection.
- While the device is powered, press the **boot button** to enter DFU mode. The red LED will start blinking periodically, indicating that the device is in DFU mode and ready to receive firmware.
- **Note**: The device need to be in **DFU Mode** to receive firmware or install drivers.

### Windows

1. Download and run [Zadig](https://zadig.akeo.ie/).
2. Plug in your BrakeBright device (see “Entering DFU Mode” 👆), then in Zadig:

   - Choose `BrakeBright Bootloader` with USB ID `1209:2444`
   - Select **WinUSB** (or **libusbK**) as the driver
   - Click **Install Driver** (you need to do this **only once**)

   ![Screenshot](screenshots/zadig.png)

3. **Reboot** your PC if needed.

4. Download the latest Bikesafe Utility `.zip` from the [Github Releases](https://github.com/mygnu/bikesafe-util/releases) page and extract.

### Linux

1. Build or download the latest release from the [GitHub Releases](https://github.com/mygnu/bikesafe-util/releases).
2. **Create a udev rule** so non-root users can access the DFU interface:

   Save the following to `/etc/udev/rules.d/70-bootloader.rules`:

   ```ini
   ATTRS{idVendor}=="1209", ATTRS{idProduct}=="2444", TAG+="uaccess"
   ```

3. Reload udev rules and trigger:

   ```bash
   sudo udevadm control --reload && sudo udevadm trigger
   ```

## Usage

### GUI

1. Launch the `bikesafe-util` executable.
2. In the file picker, select `firmware_[version].bin`.
3. Click **Update Firmware**.
4. Monitor the progress bar.
5. On success, the device will auto-exit DFU mode.

![Screenshot](screenshots/brakebrightutil.png)

### CLI

```bash
# Show help
bikesafe-util --help

# Flash firmware via CLI
bikesafe-util \
  --device 1209:2444 \
  --path firmware.bin \
  --reset
```

- `--device` (`-d`): Vendor\:Product ID
- `--path` (`-p`): path to `.bin` file
- `--reset` (`-r`): issue a detach/reset after download

## Post-Flash Test

After a successful flash, the device will exit DFU mode automatically. To verify operation:

1. Tilt the device **forward** in the direction of the arrow printed on it to simulate deceleration.
2. The red light should illuminate in a pattern resembling a brake-light signal.

## Contributing

Pull requests, issues, and feature requests are welcome. Please follow the [Rust API style guidelines](https://github.com/rust-lang/api-guidelines) and ensure CI passes.

## License

This project is licensed under the **GPLv3** license.
