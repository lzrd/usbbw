//! USB Bandwidth Visualization Tool - CLI entry point.

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{Shell, generate};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::prelude::*;
use std::io::stdout;
use std::path::PathBuf;
use std::time::Duration;

use usbbw::config::{Config, example_config, generate_config};
use usbbw::model::{BandwidthPool, format_bandwidth};
use usbbw::output::{generate_markdown, generate_mermaid};
use usbbw::sysfs::SysfsParser;
use usbbw::ui::{App, ViewMode, render};

#[derive(Parser)]
#[command(name = "usbbw")]
#[command(about = "USB bandwidth visualization tool")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Config file path (default: auto-detect)
    #[arg(short, long)]
    config: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Show bandwidth usage summary
    Summary,

    /// Generate detailed report (for sharing/debugging)
    Report,

    /// Export topology as Mermaid diagram
    Mermaid {
        /// Output file (default: stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Generate full markdown document with summary
        #[arg(long)]
        markdown: bool,

        /// Generate standalone HTML file (opens in browser)
        #[arg(long)]
        html: bool,
    },

    /// List all devices
    List {
        /// Show only devices with periodic (bandwidth-reserving) endpoints
        #[arg(long)]
        periodic_only: bool,

        /// Show verbose endpoint details
        #[arg(short, long)]
        verbose: bool,
    },

    /// Show best buses for new devices
    Recommend,

    /// Print blank example config file
    InitConfig,

    /// Generate config from current system
    GenerateConfig {
        /// Output file (default: stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Handle completions early (doesn't need config or topology)
    if let Some(Commands::Completions { shell }) = &cli.command {
        let mut cmd = Cli::command();
        generate(*shell, &mut cmd, "usbbw", &mut std::io::stdout());
        return Ok(());
    }

    // Load config
    let mut config = match &cli.config {
        Some(path) => Config::load_from_path(path)?,
        None => Config::load()?,
    };

    // Parse USB topology
    let parser = SysfsParser::new();
    let topology = parser.parse_topology()?;

    // Apply auto-detected defaults for any missing labels
    config.apply_defaults_from_topology(&topology);

    match cli.command {
        Some(Commands::Summary) => {
            print_summary(&topology, &config);
        }
        Some(Commands::Report) => {
            print_report(&topology, &config);
        }
        Some(Commands::Mermaid {
            output,
            markdown,
            html,
        }) => {
            let content = if html {
                usbbw::output::generate_html(&topology, &config)
            } else if markdown {
                generate_markdown(&topology, &config)
            } else {
                generate_mermaid(&topology, &config)
            };
            match output {
                Some(path) => std::fs::write(path, content)?,
                None => print!("{}", content),
            }
        }
        Some(Commands::List {
            periodic_only,
            verbose,
        }) => {
            print_device_list(&topology, &config, periodic_only, verbose);
        }
        Some(Commands::Recommend) => {
            print_recommendations(&topology, &config);
        }
        Some(Commands::InitConfig) => {
            print!("{}", example_config());
        }
        Some(Commands::GenerateConfig { output }) => {
            let content = generate_config(&topology);
            match output {
                Some(path) => {
                    std::fs::write(&path, &content)?;
                    eprintln!("Config written to {}", path.display());
                    eprintln!("Edit the file to customize labels, then copy to one of:");
                    eprintln!("  ./usbbw.toml");
                    eprintln!("  ~/.config/usbbw/config.toml");
                    eprintln!("  /etc/usbbw.toml");
                }
                None => print!("{}", content),
            }
        }
        Some(Commands::Completions { .. }) => {
            // Handled above before loading config/topology
            unreachable!()
        }
        None => {
            // Default: run TUI
            run_tui(topology, config)?;
        }
    }

    Ok(())
}

fn print_summary(topology: &usbbw::UsbTopology, config: &Config) {
    println!("USB Bus Bandwidth Summary");
    println!("=========================\n");

    for bus in topology.buses_sorted() {
        let pool = BandwidthPool::with_usage(bus.speed, bus.periodic_bandwidth_used_bps());
        let label = config
            .bus_label(bus.bus_num)
            .unwrap_or_else(|| format!("Bus {}", bus.bus_num));
        let bus_type = if bus.is_superspeed() {
            "USB 3.x"
        } else {
            "USB 2.0"
        };

        // Show paired bus info
        let paired_info = if let Some(paired_num) = topology.get_paired_bus(bus.bus_num) {
            format!(" ↔ Bus {}", paired_num)
        } else {
            String::new()
        };

        println!(
            "{} ({}, {}){}",
            label,
            bus_type,
            bus.speed.short_name(),
            paired_info
        );
        println!(
            "  Periodic BW: {} / {} ({:.1}%)",
            pool.format_used(),
            pool.format_max(),
            pool.periodic_usage_percent()
        );
        println!("  Available:   {}", pool.format_available());
        println!("  Devices:     {}", bus.device_count());
        let total_power = bus.total_power_ma();
        if total_power > 0 {
            println!("  Power:       {} mA", total_power);
        }
        println!();
    }
}

fn print_report(topology: &usbbw::UsbTopology, config: &Config) {
    // Collect totals
    let mut total_devices = 0;
    let mut total_periodic_bw = 0u64;
    let mut total_power_ma = 0u16;
    let mut unconfigured_count = 0;

    // Print by controller
    for controller in topology.controllers_sorted() {
        let controller_label = config
            .controller_label(&controller.id.0)
            .unwrap_or_else(|| controller.id.0.clone());

        println!("Controller: {} [{}]", controller_label, controller.id.0);

        // Get buses for this controller (USB 2.0 first, then 3.x)
        let bus_nums: Vec<u8> = [controller.usb2_bus, controller.usb3_bus]
            .into_iter()
            .flatten()
            .collect();

        for bus_num in bus_nums {
            let Some(bus) = topology.buses.get(&bus_num) else {
                continue;
            };

            let pool = BandwidthPool::with_usage(bus.speed, bus.periodic_bandwidth_used_bps());
            let bus_label = config
                .bus_label(bus.bus_num)
                .unwrap_or_else(|| format!("Bus {}", bus.bus_num));

            println!(
                "  Bus {}: {} ({}) - {:.1}% used",
                bus.bus_num,
                bus_label,
                bus.speed.short_name(),
                pool.periodic_usage_percent()
            );

            // Show port health issues
            let oc_count = bus.total_over_current_count();
            if oc_count > 0 {
                println!("    ⚠ Over-current events: {}", oc_count);
            }
            for port in &bus.ports {
                if port.state.is_problematic() {
                    let state_str = match port.state {
                        usbbw::model::PortState::Reconnecting => "reconnecting",
                        usbbw::model::PortState::Powered => "powered (not enumerated)",
                        usbbw::model::PortState::Disconnected => "disconnected",
                        _ => "issue",
                    };
                    println!("    ⚠ Port {}: {}", port.port_num, state_str);
                }
            }

            // Print devices in tree order
            for device in bus.devices_tree_order() {
                total_devices += 1;
                total_periodic_bw += device.periodic_bandwidth_bps();
                total_power_ma = total_power_ma.saturating_add(device.max_power_ma);

                let indent = "    ".to_string() + &"  ".repeat(device.path.depth());
                let name = config
                    .device_label(
                        &device.path.0,
                        device.vendor_id,
                        device.product_id,
                        device.serial.as_deref(),
                        device.physical_location.as_ref(),
                    )
                    .unwrap_or_else(|| device.display_name());

                // Status indicators
                let status = if !device.is_configured {
                    unconfigured_count += 1;
                    " [NOT CONFIGURED]"
                } else {
                    ""
                };

                // Device line: path, config key, name
                println!(
                    "{}{}  {}  {}{}",
                    indent,
                    device.path.0,
                    device.config_key(),
                    name,
                    status
                );

                // Details line: bandwidth, power
                let bw = device.periodic_bandwidth_bps();
                let mut details = Vec::new();
                if bw > 0 {
                    details.push(format!("bw:{}", format_bandwidth(bw)));
                }
                if device.max_power_ma > 0 {
                    details.push(format!("pwr:{}mA", device.max_power_ma));
                }
                if device.is_hub {
                    details.push("hub".to_string());
                }
                if !details.is_empty() {
                    println!("{}  {}", indent, details.join(" "));
                }
            }
        }
        println!();
    }

    // Summary
    println!("---");
    println!(
        "Total: {} devices, {}, {} mA",
        total_devices,
        format_bandwidth(total_periodic_bw),
        total_power_ma
    );
    if unconfigured_count > 0 {
        println!(
            "Warning: {} device(s) not configured (bandwidth allocation failed)",
            unconfigured_count
        );
    }
}

fn print_device_list(
    topology: &usbbw::UsbTopology,
    config: &Config,
    periodic_only: bool,
    verbose: bool,
) {
    for bus in topology.buses_sorted() {
        let label = config
            .bus_label(bus.bus_num)
            .unwrap_or_else(|| format!("Bus {}", bus.bus_num));
        println!("=== {} ({}) ===", label, bus.speed.short_name());

        for device in bus.devices_tree_order() {
            let has_periodic = !device.periodic_endpoints().is_empty();

            if periodic_only && !has_periodic {
                continue;
            }

            let indent = "  ".repeat(device.path.depth() + 1);
            let name = config
                .device_label(
                    &device.path.0,
                    device.vendor_id,
                    device.product_id,
                    device.serial.as_deref(),
                    device.physical_location.as_ref(),
                )
                .unwrap_or_else(|| device.display_name());

            // Show port path for root-level devices (direct on bus)
            let port_prefix = if device.path.depth() == 0 {
                format!("[{}] ", device.path.0)
            } else {
                String::new()
            };

            let icon = if !device.is_configured {
                "⚠"
            } else if device.is_hub {
                "Hub"
            } else {
                "Dev"
            };
            let status_str = if !device.is_configured {
                " [NOT CONFIGURED]".to_string()
            } else {
                let bw = device.periodic_bandwidth_bps();
                if bw > 0 {
                    format!(" [{}]", format_bandwidth(bw))
                } else {
                    String::new()
                }
            };

            println!(
                "{}{}{} {} ({}){}",
                indent,
                port_prefix,
                icon,
                name,
                device.vid_pid(),
                status_str
            );

            if verbose {
                // Show power consumption
                if device.max_power_ma > 0 {
                    println!("{}    Power: {} mA", indent, device.max_power_ma);
                }
                if let Some(serial) = &device.serial {
                    println!("{}    Serial: {}", indent, serial);
                }
                // Show physical location for root-level devices
                if device.path.depth() == 0
                    && let Some(loc) = &device.physical_location
                {
                    println!(
                        "{}    Location: panel={} vert={} horiz={}",
                        indent, loc.panel, loc.vertical_position, loc.horizontal_position
                    );
                }
                for ep in device.periodic_endpoints() {
                    let ep_bw = ep.bandwidth_bps(device.speed);
                    println!(
                        "{}    EP{:02X} {} {} {}B @ {} -> {}",
                        indent,
                        ep.address,
                        ep.transfer_type,
                        ep.direction,
                        ep.max_packet_size,
                        ep.interval_str,
                        format_bandwidth(ep_bw)
                    );
                }
            }
        }
        println!();
    }
}

fn print_recommendations(topology: &usbbw::UsbTopology, config: &Config) {
    println!("Best Buses for New Devices");
    println!("==========================\n");
    println!("Note: Bandwidth is shared across the entire bus, not per-hub.");
    println!("All devices behind a hub share the bus bandwidth pool.\n");

    // Sort buses by available bandwidth
    let mut buses: Vec<_> = topology.buses_sorted();
    buses.sort_by(|a, b| {
        let a_avail = a.speed.max_periodic_bandwidth_bps() - a.periodic_bandwidth_used_bps();
        let b_avail = b.speed.max_periodic_bandwidth_bps() - b.periodic_bandwidth_used_bps();
        b_avail.cmp(&a_avail)
    });

    // Group by USB 2.0 and USB 3.x
    println!("USB 3.x Buses (SuperSpeed):");
    for bus in buses.iter().filter(|b| b.is_superspeed()) {
        let pool = BandwidthPool::with_usage(bus.speed, bus.periodic_bandwidth_used_bps());
        let label = config
            .bus_label(bus.bus_num)
            .unwrap_or_else(|| format!("Bus {}", bus.bus_num));
        println!(
            "  {} - {} available ({:.1}% used)",
            label,
            pool.format_available(),
            pool.periodic_usage_percent()
        );
    }

    println!("\nUSB 2.0 Buses (High Speed):");
    for bus in buses.iter().filter(|b| !b.is_superspeed()) {
        let pool = BandwidthPool::with_usage(bus.speed, bus.periodic_bandwidth_used_bps());
        let label = config
            .bus_label(bus.bus_num)
            .unwrap_or_else(|| format!("Bus {}", bus.bus_num));
        println!(
            "  {} - {} available ({:.1}% used)",
            label,
            pool.format_available(),
            pool.periodic_usage_percent()
        );
    }
}

fn run_tui(topology: usbbw::UsbTopology, config: Config) -> Result<()> {
    // Initialize terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let refresh_ms = config.settings.refresh_ms;
    let mut app = App::new(topology, config);

    loop {
        terminal.draw(|f| render(f, &app))?;

        // Poll for events with timeout for auto-refresh
        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            // Handle edit mode separately
            if app.edit_mode.is_some() {
                match key.code {
                    KeyCode::Enter => {
                        app.confirm_edit();
                    }
                    KeyCode::Esc => {
                        app.cancel_edit();
                    }
                    KeyCode::Backspace => {
                        if let Some(edit) = &mut app.edit_mode {
                            edit.input.pop();
                            edit.cursor = edit.input.len();
                        }
                    }
                    KeyCode::Char(c) => {
                        if let Some(edit) = &mut app.edit_mode {
                            edit.input.push(c);
                            edit.cursor = edit.input.len();
                        }
                    }
                    _ => {}
                }
                continue;
            }

            // Normal mode keybindings
            match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Char('j') | KeyCode::Down => {
                    app.move_selection(1);
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    app.move_selection(-1);
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    app.toggle_expand();
                }
                KeyCode::Char('g') => {
                    app.goto_top();
                }
                KeyCode::Char('G') => {
                    app.goto_bottom();
                }
                KeyCode::Char('t') => {
                    app.set_view_mode(ViewMode::Tree);
                }
                KeyCode::Char('s') => {
                    app.set_view_mode(ViewMode::Summary);
                }
                KeyCode::Char('?') => {
                    app.show_help = !app.show_help;
                }
                KeyCode::Char('a') => {
                    app.auto_refresh = !app.auto_refresh;
                }
                KeyCode::Char('r') => {
                    // Manual refresh
                    let parser = SysfsParser::new();
                    if let Ok(new_topology) = parser.parse_topology() {
                        app.update_topology(new_topology);
                    }
                }
                KeyCode::Char('b') => {
                    // Toggle bandwidth bars
                    app.toggle_bandwidth_bars();
                }
                KeyCode::Char('x') => {
                    // Toggle expand all / collapse all
                    app.toggle_expand_all();
                }
                KeyCode::Char('e') => {
                    // Edit label for selected device
                    if app.selected_device.is_some() {
                        app.start_edit();
                    }
                }
                KeyCode::Char('m') => {
                    // Mark selected device as seen
                    if let Some(path) = &app.selected_device {
                        app.mark_seen(&path.0.clone());
                    }
                }
                KeyCode::Char('w') => {
                    // Write pending labels to config
                    if app.pending_label_count() > 0 {
                        match write_pending_labels(&app) {
                            Ok(path) => {
                                let count = app.pending_label_count();
                                // Merge pending labels into config so they persist in display
                                for (key, label) in app.pending_labels.drain() {
                                    app.config.products.insert(key, label);
                                }
                                app.set_status(format!(
                                    "Wrote {} label(s) to {}",
                                    count,
                                    path.display()
                                ));
                            }
                            Err(e) => {
                                app.set_status(format!("Error writing config: {}", e));
                            }
                        }
                    }
                }
                KeyCode::Esc => {
                    if app.show_help {
                        app.show_help = false;
                    }
                }
                KeyCode::PageUp | KeyCode::Char('K') => {
                    app.scroll_details_up();
                }
                KeyCode::PageDown | KeyCode::Char('J') => {
                    app.scroll_details_down();
                }
                _ => {}
            }
        }

        // Auto-refresh
        if app.auto_refresh && app.last_refresh.elapsed().as_millis() > refresh_ms as u128 {
            let parser = SysfsParser::new();
            if let Ok(new_topology) = parser.parse_topology() {
                app.update_topology(new_topology);
            }
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    Ok(())
}

/// Write pending labels to the user's config file.
fn write_pending_labels(app: &App) -> Result<std::path::PathBuf> {
    use std::fs;
    use std::io::Write;

    // Determine config path (prefer user config dir)
    let config_dir = dirs::config_dir()
        .map(|d| d.join("usbbw"))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let config_path = config_dir.join("config.toml");

    // Ensure directory exists
    fs::create_dir_all(&config_dir)?;

    // Read existing config or create new
    let mut content = if config_path.exists() {
        fs::read_to_string(&config_path)?
    } else {
        String::from("# usbbw configuration\n\n")
    };

    // Check if [products] section exists
    let has_products_section = content.contains("[products]");

    if !has_products_section {
        content.push_str("\n[products]\n");
    }

    // Append new product labels
    // Find the end of the [products] section or end of file
    let insert_pos = if has_products_section {
        // Find position after [products] line
        if let Some(pos) = content.find("[products]") {
            // Find next section or end of file
            let after_products = &content[pos + 10..];
            if let Some(next_section) = after_products.find("\n[") {
                pos + 10 + next_section
            } else {
                content.len()
            }
        } else {
            content.len()
        }
    } else {
        content.len()
    };

    // Build new entries (VID:PID:iSerial or VID:PID)
    let mut new_entries = String::new();
    for (product_key, label) in &app.pending_labels {
        // Escape the label for TOML
        let escaped = label.replace('\\', "\\\\").replace('"', "\\\"");
        new_entries.push_str(&format!("\"{}\" = \"{}\"\n", product_key, escaped));
    }

    // Insert at the right position
    content.insert_str(insert_pos, &new_entries);

    // Write back
    let mut file = fs::File::create(&config_path)?;
    file.write_all(content.as_bytes())?;

    Ok(config_path)
}
