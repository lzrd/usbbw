# usbbw

A terminal UI for visualizing USB bandwidth allocation on Linux.

![Rust](https://img.shields.io/badge/rust-stable-orange)
![Platform](https://img.shields.io/badge/platform-linux-blue)

## Features

- **Tree view** of USB topology (controllers, buses, hubs, devices)
- **Bandwidth tracking** for periodic transfers (interrupt/isochronous endpoints)
- **Unconfigured device detection** - shows devices that failed bandwidth allocation
- **Power consumption** display per device and bus
- **New device detection** with visual indicators when devices are plugged in
- **In-app labeling** to tag devices with portable VID:PID:iSerial keys
- **Mermaid diagram export** for documentation (markdown or standalone HTML)
- **Config inheritance** for shareable hardware-specific baselines

## Installation

```bash
cargo install --path .
```

Or build from source:

```bash
cargo build --release
./target/release/usbbw
```

## Usage

```bash
usbbw                       # Launch TUI (default)
usbbw summary               # Text summary of bandwidth per bus
usbbw list [-v]             # List devices (verbose shows power, serial)
usbbw recommend             # Show best buses for new devices
usbbw mermaid               # Export Mermaid diagram
usbbw mermaid --markdown    # Full markdown doc with tables
usbbw mermaid --html        # Standalone HTML (view in browser)
usbbw generate-config       # Generate config from current system
usbbw completions bash      # Generate shell completions
```

## TUI Keybindings

| Key | Action |
|-----|--------|
| `j/k` | Navigate up/down |
| `Enter` | Expand/collapse |
| `x` | Expand/collapse all |
| `t/s` | Tree/Summary view |
| `b` | Toggle bandwidth bars |
| `e` | Edit device label |
| `m` | Mark device as seen |
| `w` | Write labels to config |
| `r` | Refresh |
| `?` | Help |
| `q` | Quit |

## Device Labeling

Labels are stored by `VID:PID:iSerial` so they follow devices across ports:

```toml
[products]
"0d28:0204:02400000e428" = "Sidecar RoT"  # Specific device (has serial)
"0d28:0204" = "OxLink"                     # All devices of this type
```

Press `e` in the TUI to label a device, then `w` to save to config.

## Configuration

Config files are searched in order:
1. `./usbbw.toml`
2. `~/.config/usbbw/config.toml`
3. `/etc/usbbw.toml`

### Example Config

```toml
# Inherit from a hardware baseline
inherit = "configs/framework-franmgcp.toml"

[products]
"0d28:0204" = "Debug Probe"
"0d28:0204:SERIAL123" = "Specific Probe"
```

### Position Labels

Map ACPI physical_location values to friendly names:

```toml
[position_labels.vertical]
upper = "Rear"
lower = "Front"

[position_labels.panel]
left = "Left"
right = "Right"
```

## USB Bandwidth Model

- Only **interrupt** and **isochronous** endpoints reserve bandwidth
- USB 2.0: Max 80% of 480 Mbps = 384 Mbps for periodic transfers
- USB 3.x: Separate bandwidth pool from USB 2.0
- Devices with `⚠ [NOT CONFIGURED]` failed bandwidth allocation

## Shell Completions

```bash
# Bash
usbbw completions bash > ~/.local/share/bash-completion/completions/usbbw.sh

# Zsh
usbbw completions zsh > ~/.zfunc/_usbbw

# Fish
usbbw completions fish > ~/.config/fish/completions/usbbw.fish
```

## License

MIT
