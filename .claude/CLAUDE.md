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
│   ├── app.rs          # App state, tree navigation
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
- `UsbDevice` - A device with endpoints, supports hubs
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
usbbw summary                   # Text summary of bus bandwidth + power
usbbw recommend                 # Show best buses for new devices
usbbw list [-v] [--periodic-only]  # List devices (verbose shows power, serial)
usbbw mermaid [--markdown]      # Export Mermaid diagram
usbbw init-config               # Print blank example TOML config
usbbw generate-config [-o FILE] # Generate config from current system
```

## TUI Keybindings

- `j/k` or arrows: Navigate
- `Enter`: Expand/collapse
- `t`: Tree view
- `s`: Summary view
- `r`: Refresh
- `a`: Toggle auto-refresh
- `?`: Help
- `q`: Quit

## Configuration

### Config File Locations

Searched in order:
1. `./usbbw.toml`
2. `~/.config/usbbw/config.toml`
3. `/etc/usbbw.toml`

### Auto-generated Labels

If no config file exists, labels are automatically generated from the detected USB topology:
- Controllers: "USB Controller"
- Buses: "Bus N"
- Physical ports: Based on ACPI `physical_location` attributes
- Products: From USB descriptor product/manufacturer strings

Use `usbbw generate-config` to create a starter config file.

### Position Label Mappings

ACPI `physical_location` values can be mapped to user-friendly names via config:

```toml
[position_labels.vertical]
upper = "Rear"    # Near hinge on laptops
lower = "Front"   # Near front edge

[position_labels.panel]
left = "Left"
right = "Right"
```

This is useful because ACPI uses generic terms like "upper/lower" which may not match intuitive port naming (e.g., on Framework laptops, "upper" means "rear").

### Config Inheritance

Configs can inherit from other configs using the `inherit` key:

```toml
# Inherit from a baseline config
inherit = "../configs/framework-franmgcp.toml"

# Or inherit from multiple files (applied in order)
inherit = ["base.toml", "overrides.toml"]

# Then add your customizations
[products]
"0d28:0204" = "My Debug Probe"
```

**Merge behavior:**
- Tables are merged recursively (child values override parent)
- Arrays are concatenated
- Scalar values are replaced by the child

Paths are relative to the config file containing the `inherit` key.

### Baseline Configs

Baseline configs for known hardware are in `configs/`:
- `framework-franmgcp.toml` - Framework Laptop 13 AMD Ryzen AI 300 Series

These provide position label mappings, controller labels, and port capability documentation specific to the hardware. User configs can inherit from these baselines.

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

[devices]
"3-1" = "TB Hub"

[mermaid]
hide_paths = []
filter_vendors = []
collapse_single_child_hubs = false
```

## Dependencies

- `clap` - CLI argument parsing
- `ratatui` + `crossterm` - TUI framework
- `serde` + `toml` - Configuration
- `thiserror` + `anyhow` - Error handling
- `dirs` - XDG directory support
