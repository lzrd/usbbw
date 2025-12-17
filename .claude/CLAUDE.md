# usbbw - USB Bandwidth Visualization Tool

## Overview

A Rust CLI tool for visualizing USB bandwidth allocation on Linux systems. It reads from `/sys/bus/usb/devices/` to parse the USB topology and calculates periodic bandwidth usage for each bus.

## Build Commands

```bash
cargo build              # Debug build
cargo build --release    # Release build
cargo test               # Run tests
cargo fmt                # Format code
cargo clippy             # Run linter (fix all warnings)
```

## Project Structure

```
src/
├── lib.rs              # Library exports
├── main.rs             # CLI entry point (clap, TUI runner)
├── model/              # Business logic - data structures
│   ├── speed.rs        # UsbSpeed enum (Low/Full/High/Super/SuperPlus)
│   ├── endpoint.rs     # Endpoint, TransferType, bandwidth calculation
│   ├── topology.rs     # UsbTopology, UsbBus, UsbDevice, DevicePath
│   └── bandwidth.rs    # BandwidthPool, formatting helpers
├── sysfs/              # Linux sysfs parsing
│   └── parser.rs       # SysfsParser - reads /sys/bus/usb/devices/
├── config/             # Configuration
│   └── loader.rs       # TOML config loading, label resolution
├── ui/                 # TUI (ratatui)
│   ├── app.rs          # App state, tree navigation, device tracking
│   └── render.rs       # Tree view, details panel, help overlay
└── output/             # Non-TUI output
    └── mermaid.rs      # Mermaid diagram generator

configs/                # Baseline configs for known hardware
├── framework-franmgcp.toml  # Framework Laptop 13 AMD Ryzen AI 300

examples/
└── ox.toml             # Example user config (Oxide dev setup)
```

## Architecture

**Separation of concerns:**
- `model/` contains pure data structures and calculations - no I/O
- `sysfs/` handles all Linux-specific filesystem access
- `config/` handles TOML parsing and label resolution
- `ui/` contains ratatui-specific rendering code
- `main.rs` only handles CLI argument parsing and terminal setup

**Key types:**
- `UsbTopology` - Complete system USB topology (controllers + buses + devices)
- `UsbBus` - A USB bus with its devices and bandwidth pool
- `UsbDevice` - A device with endpoints, configuration status, supports hubs
  - `config_key()` - Returns `VID:PID:iSerial` or `VID:PID` for config lookups
  - `vid_pid()` - Returns `VID:PID` formatted string
- `Endpoint` - USB endpoint with bandwidth calculation
- `BandwidthPool` - Tracks used/available periodic bandwidth

## USB Bandwidth Model

- Only **Interrupt** and **Isochronous** endpoints reserve bandwidth
- Bulk and Control transfers use spare bandwidth
- USB 2.0: Max 80% of 480 Mbps = 384 Mbps for periodic transfers
- USB 3.x: Separate bandwidth pool from USB 2.0
- xHCI controllers pair USB 2.0 + USB 3.x buses (odd = 2.0, even = 3.x)

**Bandwidth calculation:**
```
bandwidth_bps = (max_packet_size * multiplier * 8) / interval_us * 1_000_000
```

## Unconfigured Device Detection

Devices that fail bandwidth allocation are detected via sysfs:
- `bConfigurationValue` empty or 0 indicates failed configuration
- Shown with ⚠ icon and `[NOT CONFIGURED]` in red
- No root/dmesg required - uses sysfs attributes

## Power Consumption

Each device reports its configured maximum power consumption (`bMaxPower` from USB descriptors):
- Shown per-device in `list -v` output
- Shown per-bus total in `summary` output
- Note: This is configured max, not actual measured consumption

## Code Style

- Run `cargo fmt` before committing
- Run `cargo clippy` and fix all warnings
- Use `thiserror` for error types
- Prefer `Option::map` over `if let Some`
- Use let-chains for nested conditionals: `if let Some(x) = foo && condition { }`

## Testing

Tests are in `#[cfg(test)]` modules within each file. Run with:
```bash
cargo test
```

To test with a config file:
```bash
./target/release/usbbw -c examples/ox.toml summary
```

## CLI Commands

```bash
usbbw                           # Launch TUI (default)
usbbw report                    # Detailed report for sharing/debugging (diffable)
usbbw summary                   # Text summary of bus bandwidth + power
usbbw recommend                 # Show best buses for new devices
usbbw list [-v] [--periodic-only]  # List devices (verbose shows power, serial)
usbbw mermaid                   # Export Mermaid diagram
usbbw mermaid --markdown        # Full markdown doc with tables
usbbw mermaid --html            # Standalone HTML (view in browser)
usbbw init-config               # Print blank example TOML config
usbbw generate-config [-o FILE] # Generate config from current system
usbbw completions <SHELL>       # Generate shell completions (bash/zsh/fish/powershell/elvish)
```

## TUI Keybindings

**Navigation:**
- `j/k` or arrows: Navigate up/down
- `J/K` or PgUp/PgDn: Scroll details panel
- `Enter`: Expand/collapse selected
- `x`: Expand/collapse all
- `g`: Go to top
- `G`: Go to bottom

**Views:**
- `t`: Tree view
- `s`: Summary view
- `b`: Toggle bandwidth bars

**Device Labels:**
- `e`: Edit label for selected device
- `m`: Mark device as seen (clear NEW indicator)
- `w`: Write pending labels to config

**Other:**
- `r`: Refresh topology
- `a`: Toggle auto-refresh
- `?`: Help overlay
- `q`: Quit

**Status Line:**
- Shows device path (e.g., `3-2.1`) and config key (`VID:PID:iSerial`) for easy copying
- Device path format matches `uhubctl` for power control

**Tree Icons:**
- `⚡` - USB bus
- `🔀` - Hub
- `📱` - Device
- `⚠` - Device not configured (bandwidth allocation failed)
- `●NEW` - Device discovered after startup

## Device Labeling

Labels are stored by `VID:PID:iSerial` for portability across USB ports:

```toml
[products]
"0d28:0204:0240000034e428" = "Sidecar RoT"  # Specific device (has serial)
"0d28:0204" = "OxLink"                       # All devices of this type
```

**Label lookup priority:**
1. `VID:PID:iSerial` (specific device with serial)
2. `VID:PID` (all devices of this type)
3. Physical location match
4. Device path (legacy)

**In-TUI labeling:**
- Press `e` on a device to edit its label
- Press `w` to write pending labels to `~/.config/usbbw/config.toml`
- Labels are written to `[products]` section

## New Device Detection

The TUI tracks devices discovered during the session:
- `●NEW` indicator for devices not present at startup
- `[N]` shows discovery order
- Press `m` to mark as seen (clears indicator without labeling)
- Press `e` to label (also clears indicator)

## Configuration

### Config File Locations

Searched in order:
1. `./usbbw.toml`
2. `~/.config/usbbw/config.toml`
3. `/etc/usbbw.toml`

### Config Inheritance

Configs can inherit from other configs using the `inherit` key:

```toml
inherit = "../configs/framework-franmgcp.toml"

[products]
"0d28:0204" = "My Debug Probe"
```

**Merge behavior:**
- Tables are merged recursively (child values override parent)
- Arrays are concatenated
- Scalar values are replaced by the child

### Position Label Mappings

ACPI `physical_location` values can be mapped to user-friendly names:

```toml
[position_labels.vertical]
upper = "Rear"    # Near hinge on laptops
lower = "Front"   # Near front edge

[position_labels.panel]
left = "Left"
right = "Right"
```

### Config Sections

```toml
[settings]
refresh_ms = 1000
theme = "dark"
use_bits = true

[position_labels.vertical]
upper = "Rear"
lower = "Front"

[controllers]
"0000:c1:00.4" = "Internal USB"

[buses]
"1" = "Internal 2.0"

[[physical_ports]]
panel = "left"
vertical_position = "upper"
label = "Left Rear (USB4)"

[products]
"0d28:0204" = "Debug Probe"
"0d28:0204:SERIAL123" = "Specific Probe"

[devices]
"3-1" = "TB Hub"

[mermaid]
hide_paths = []
filter_vendors = []
collapse_single_child_hubs = false
```

## Shell Completions

```bash
usbbw completions bash > ~/.local/share/bash-completion/completions/usbbw.sh
usbbw completions zsh > ~/.zfunc/_usbbw
usbbw completions fish > ~/.config/fish/completions/usbbw.fish
usbbw completions powershell > usbbw.ps1
usbbw completions elvish > ~/.elvish/lib/usbbw.elv
```

## Dependencies

- `clap` + `clap_complete` - CLI argument parsing and completions
- `ratatui` + `crossterm` - TUI framework
- `serde` + `toml` - Configuration
- `thiserror` + `anyhow` - Error handling
- `dirs` - XDG directory support
