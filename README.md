# MSI Center Linux

<p align="center">
  <img src="https://img.shields.io/badge/Version-1.0.0-blue.svg" alt="Version">
  <img src="https://img.shields.io/badge/License-MIT-green.svg" alt="License">
  <img src="https://img.shields.io/badge/Platform-Linux-orange.svg" alt="Platform">
  <img src="https://img.shields.io/badge/Language-Rust-red.svg" alt="Language">
</p>

A powerful MSI laptop control center for Linux - Fan control, user scenarios, and performance profiles.

**Developed by Dasun Sanching**

---

## 📥 Download

### Pre-built Package (Recommended)

Download the latest `.deb` package from the [Releases](https://github.com/Ds-Dev-999/msi-center-linux/releases) page:

```bash
# Download and install
wget https://github.com/Ds-Dev-999/msi-center-linux/releases/download/v1.0.0/msi-center-linux_1.0.0_amd64.deb
sudo dpkg -i msi-center-linux_1.0.0_amd64.deb
```

Or download directly: **[msi-center-linux_1.0.0_amd64.deb](https://github.com/Ds-Dev-999/msi-center-linux/releases/download/v1.0.0/msi-center-linux_1.0.0_amd64.deb)**

---

## ✨ Features

- **Fan Control**
  - Read CPU/GPU fan speeds and temperatures
  - Set fan modes (Auto, Silent, Basic, Advanced)
  - Custom fan curves with temperature-based speed control
  - Manual fan speed override
  - Cooler Boost toggle

- **User Scenarios**
  - Silent - Low noise, reduced performance
  - Balanced - Default balanced mode
  - High Performance - Maximum CPU/GPU performance
  - Turbo - Extreme performance with Cooler Boost
  - Super Battery - Maximum battery life

- **Profile Management**
  - Create and save custom profiles
  - Quick profile switching
  - Persistent configuration

- **Real-time Monitoring**
  - Live temperature and fan speed display
  - Color-coded status indicators

## Installation

### Prerequisites

- Rust toolchain (1.70+)
- Linux with MSI laptop
- Root access (for EC control)

### Build from Source

```bash
git clone https://github.com/yourusername/msi-center-linux.git
cd msi-center-linux
cargo build --release
```

### Install

```bash
sudo cp target/release/msi-center /usr/local/bin/
sudo cp target/release/msi-center-gui /usr/local/bin/
```

## GUI Application

Launch the graphical interface:

```bash
sudo msi-center-gui
```

### GUI Features

- **Dashboard** - Real-time temperature and fan speed monitoring with progress bars
- **Fan Control** - Set fan modes, cooler boost, manual speeds, and custom curves
- **Scenarios** - One-click scenario switching (Silent, Balanced, Performance, Turbo, Battery)
- **Profiles** - Create, save, and manage custom profiles
- **Settings** - Application configuration and system info

## Usage

**Note:** Most commands require root privileges to access the Embedded Controller.

### Show System Status

```bash
sudo msi-center status
```

### Fan Control

```bash
# Show fan status
sudo msi-center fan status

# Set fan mode
sudo msi-center fan mode auto|silent|basic|advanced

# Enable/disable cooler boost
sudo msi-center fan cooler-boost on|off

# Set manual fan speed (0-100%)
sudo msi-center fan speed --cpu 50 --gpu 60

# Set fan curve preset
sudo msi-center fan curve --fan cpu --preset silent|balanced|performance

# Set custom fan curve
sudo msi-center fan curve --fan cpu --preset custom --points "40:0,50:30,60:50,70:70,80:90,90:100"

# Reset to automatic control
sudo msi-center fan reset
```

### User Scenarios

```bash
# Show current scenario
sudo msi-center scenario status

# List available scenarios
sudo msi-center scenario list

# Set scenario
sudo msi-center scenario set silent|balanced|highperf|turbo|battery

# Set shift mode directly
sudo msi-center scenario shift eco|comfort|sport|turbo

# Toggle super battery mode
sudo msi-center scenario super-battery on|off
```

### Profile Management

```bash
# List profiles
msi-center profile list

# Show active profile
msi-center profile active

# Set active profile
msi-center profile set "Profile Name"

# Create new profile
msi-center profile create "Gaming" --base highperf

# Delete profile
msi-center profile delete "Profile Name"

# Save current settings
msi-center profile save
```

### Real-time Monitor

```bash
sudo msi-center monitor --interval 2
```

### Apply Active Profile

```bash
sudo msi-center apply
```

## Configuration

Configuration is stored in `~/.config/msi-center-linux/config.json`.

## How It Works

MSI Center Linux interfaces with the laptop's Embedded Controller (EC) to control hardware features. It supports multiple access methods:

1. **Direct Port Access** (`/dev/port`) - Most reliable, requires root
2. **ACPI EC Interface** (`/sys/kernel/debug/ec/ec0/io`) - Requires debugfs
3. **MSI-EC Kernel Module** (`/sys/devices/platform/msi-ec`) - If available

## Supported Hardware

This tool is designed for MSI laptops with compatible EC firmware. Tested models include:

- MSI GF/GS/GP/GT series gaming laptops
- MSI Modern/Prestige series
- MSI Stealth/Raider series

## Troubleshooting

### Permission Denied

Run with `sudo` or add your user to appropriate groups:

```bash
sudo chmod 666 /dev/port  # Temporary
```

### EC Not Supported

- Ensure you have an MSI laptop
- Try loading the `ec_sys` kernel module: `sudo modprobe ec_sys`
- Check if `/sys/kernel/debug/ec/ec0/io` exists

### Values Not Changing

- Some laptops require specific BIOS settings
- Try different access methods
- Check BIOS for "EC Lock" or similar settings

## Safety Warning

**Modifying EC settings can potentially damage hardware if used incorrectly.** This software:

- Uses known-safe EC addresses documented by the MSI community
- Includes safety bounds on fan speeds and temperatures
- Should still be used with caution

The author is not responsible for any damage caused by using this software.

## 📜 License

MIT License - © 2025 Dasun Sanching

## 🤝 Contributing

Contributions welcome! Please test on your specific MSI laptop model and report compatibility.

## 🙏 Acknowledgments

- MSI-EC kernel module developers
- Linux hardware community
- isw (Ice-Sealed Wyvern) project for EC documentation

---

## 👨‍💻 Developer

**Dasun Sanching**

- Built with ❤️ using Rust & egui
- For MSI laptop users on Linux

---

<p align="center">
  <strong>⭐ Star this repo if you find it useful!</strong>
</p>
