# usbbw

A terminal UI for visualizing USB bandwidth allocation on Linux.

Working on embedded programming with multiple USB debug probes connected to my
Linux machine, I was often frustrated by the "USB bandwidth is not sufficient"
error. This program gives me immediate feedback on what devices are connected
and their USB resources consumed.

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
usbbw report                # Detailed report for sharing/debugging (diffable)
usbbw summary               # Text summary of bandwidth per bus
usbbw list [-v]             # List devices (verbose shows power, serial)
usbbw list --periodic-only  # Show only bandwidth-reserving devices
usbbw recommend             # Show best buses for new devices
usbbw mermaid               # Export Mermaid diagram
usbbw mermaid --markdown    # Full markdown doc with tables
usbbw mermaid --html        # Standalone HTML (view in browser)
usbbw init-config           # Print blank example config
usbbw generate-config       # Generate config from current system
usbbw completions <SHELL>   # Generate shell completions
```

## TUI Keybindings

| Key | Action |
|-----|--------|
| `j/k` | Navigate up/down |
| `J/K` | Scroll details panel |
| `Enter` | Expand/collapse |
| `x` | Expand/collapse all |
| `g/G` | Go to top/bottom |
| `t/s` | Tree/Summary view |
| `b` | Toggle bandwidth bars |
| `e` | Edit device label |
| `m` | Mark device as seen |
| `w` | Write labels to config |
| `r` | Refresh |
| `a` | Toggle auto-refresh |
| `?` | Help |
| `q` | Quit |

## Tree Icons

| Icon | Meaning |
|------|---------|
| `⚡` | USB bus |
| `🔀` | USB hub |
| `📱` | USB device |
| `⚠` | Device not configured (bandwidth allocation failed) |
| `●NEW` | Device discovered after startup |

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

### Config Inheritance

Configs can inherit from hardware-specific baselines using the `inherit` key:

```toml
inherit = "configs/framework-franmgcp.toml"

[products]
"0d28:0204" = "Debug Probe"
```

Inheritance behavior:
- **Tables** are merged recursively (your values override inherited ones)
- **Arrays** are concatenated
- **Scalars** are replaced by your values
- Paths are relative to the config file's directory
- Multiple inheritance: `inherit = ["base.toml", "overlay.toml"]`

This lets you share hardware-specific controller/bus labels across a team while
keeping personal device labels in your own config.

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

## Why USB Bandwidth Errors Happen

USB 2.0 reserves up to 80% of its 480 Mbps for **periodic transfers** (interrupt
and isochronous endpoints). Debug probes, audio interfaces, and webcams all
compete for this limited pool. When it's exhausted, new devices fail to
configure—shown in usbbw with `⚠ [NOT CONFIGURED]`.

Common culprits:
- **Debug probes** - CMSIS-DAP and similar use interrupt endpoints
- **Audio interfaces** - isochronous transfers for real-time audio
- **Webcams** - isochronous video streams consume significant bandwidth
- **USB hubs** - concentrate all downstream devices onto one bus

Note: USB 3.x has a separate bandwidth pool from USB 2.0 on xHCI controllers.

## Tips for Avoiding Bandwidth Errors

1. **Spread devices across controllers** - Use `usbbw recommend` to find the
   least-loaded bus before plugging in a new device

2. **Use USB 3.x ports for USB 3.x devices** - USB 2.0 and 3.x have separate
   bandwidth pools on xHCI controllers

3. **Check which devices reserve bandwidth** - Run `usbbw list --periodic-only`
   to see which devices are consuming periodic bandwidth

4. **Unplug unused devices** - Webcams and audio interfaces reserve bandwidth
   even when idle

5. **Use multiple USB controllers** - PCIe USB cards add independent bandwidth
   pools

## Shell Completions

```bash
# Bash
usbbw completions bash > ~/.local/share/bash-completion/completions/usbbw.sh

# Zsh
usbbw completions zsh > ~/.zfunc/_usbbw

# Fish
usbbw completions fish > ~/.config/fish/completions/usbbw.fish

# PowerShell
usbbw completions powershell > usbbw.ps1

# Elvish
usbbw completions elvish > ~/.elvish/lib/usbbw.elv
```

## License

MIT
