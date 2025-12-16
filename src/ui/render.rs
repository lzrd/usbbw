//! TUI rendering with ratatui.

use crate::model::{BandwidthPool, bandwidth::bandwidth_bar, format_bandwidth};
use crate::ui::app::{App, TreeItem, ViewMode};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Padding, Paragraph, Wrap},
};

/// Main render function.
pub fn render(frame: &mut Frame, app: &App) {
    // Check if we're in edit mode - if so, render edit overlay and return
    if app.edit_mode.is_some() {
        render_with_edit_overlay(frame, app);
        return;
    }

    // Main layout: content area + footer
    let outer_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(frame.area());

    // Content area: tree on left, details on right
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(outer_chunks[0]);

    // Left side: tree view or summary
    match app.view_mode {
        ViewMode::Tree => render_tree(frame, app, main_chunks[0]),
        ViewMode::Summary => render_summary(frame, app, main_chunks[0]),
    }

    // Right side: details
    render_details(frame, app, main_chunks[1]);

    // Footer with contextual keybindings
    render_footer(frame, app, outer_chunks[1]);

    // Help overlay if active
    if app.show_help {
        render_help(frame);
    }
}

/// Render tree view.
fn render_tree(frame: &mut Frame, app: &App, area: Rect) {
    let items = app.visible_items();

    let list_items: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let indent = "  ".repeat(item.depth());
            let is_selected = i == app.selected;

            let mut spans = vec![Span::raw(indent)];

            match item {
                TreeItem::Controller { id, label, .. } => {
                    let expanded = app.is_expanded(id);
                    let prefix = if expanded { "▼ " } else { "▶ " };
                    let mut style = Style::default().fg(Color::Cyan);
                    if is_selected {
                        style = style.bg(Color::DarkGray).add_modifier(Modifier::BOLD);
                    }
                    spans.push(Span::raw(prefix));
                    spans.push(Span::styled(label.clone(), style));
                }
                TreeItem::Bus {
                    bus_num,
                    speed_name,
                    usage_percent,
                    label,
                    ..
                } => {
                    let key = format!("bus{}", bus_num);
                    let expanded = app.is_expanded(&key);
                    let prefix = if expanded { "├─▼ " } else { "├─▶ " };
                    let mut style = Style::default().fg(Color::Yellow);
                    if is_selected {
                        style = style.bg(Color::DarkGray).add_modifier(Modifier::BOLD);
                    }
                    let name = label.clone().unwrap_or_else(|| format!("Bus {}", bus_num));

                    spans.push(Span::raw(prefix));
                    spans.push(Span::styled(format!("⚡ {} ({})", name, speed_name), style));

                    // Optional inline bandwidth bar
                    if app.show_bandwidth_bars {
                        let bar = bandwidth_bar(*usage_percent, 10);
                        let bar_color = if *usage_percent > 80.0 {
                            Color::Red
                        } else if *usage_percent > 50.0 {
                            Color::Yellow
                        } else {
                            Color::Green
                        };
                        spans.push(Span::raw(" "));
                        spans.push(Span::styled(bar, Style::default().fg(bar_color)));
                        spans.push(Span::styled(
                            format!(" {:.0}%", usage_percent),
                            Style::default().fg(bar_color),
                        ));
                    } else {
                        spans.push(Span::styled(
                            format!(" [{:.1}%]", usage_percent),
                            Style::default().fg(Color::DarkGray),
                        ));
                    }
                }
                TreeItem::Device {
                    label,
                    is_hub,
                    has_children,
                    bandwidth_bps,
                    is_new,
                    discovery_number,
                    is_configured,
                    ..
                } => {
                    let prefix = if *is_hub && *has_children {
                        "├─○ "
                    } else {
                        "└── "
                    };
                    let icon = if !is_configured {
                        "⚠"
                    } else if *is_hub {
                        "🔀"
                    } else {
                        "📱"
                    };

                    let mut style = Style::default();
                    if !is_configured {
                        style = style.fg(Color::Red);
                    }
                    if is_selected {
                        style = style.bg(Color::DarkGray).add_modifier(Modifier::BOLD);
                    }

                    spans.push(Span::raw(prefix));
                    spans.push(Span::styled(format!("{} {}", icon, label), style));

                    // NOT CONFIGURED indicator or bandwidth info
                    if !is_configured {
                        spans.push(Span::styled(
                            " [NOT CONFIGURED]",
                            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                        ));
                    } else if *bandwidth_bps > 0 {
                        spans.push(Span::styled(
                            format!(" [{}]", format_bandwidth(*bandwidth_bps)),
                            Style::default().fg(Color::DarkGray),
                        ));
                    }

                    // NEW indicator
                    if *is_new {
                        spans.push(Span::styled(
                            " ●NEW",
                            Style::default()
                                .fg(Color::LightGreen)
                                .add_modifier(Modifier::BOLD),
                        ));
                        // Discovery order number
                        if let Some(n) = discovery_number {
                            spans.push(Span::styled(
                                format!(" [{}]", n),
                                Style::default().fg(Color::LightGreen),
                            ));
                        }
                    }
                }
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    // Build title with new device count
    let new_count = app.new_device_count();
    let pending_count = app.pending_label_count();
    let title = if new_count > 0 || pending_count > 0 {
        let mut parts = vec![format!(" USB Topology ({})", app.device_count_str())];
        if new_count > 0 {
            parts.push(format!("{} new", new_count));
        }
        if pending_count > 0 {
            parts.push(format!("{} pending", pending_count));
        }
        parts.push(format!(
            "[{}] ",
            if app.auto_refresh { "auto" } else { "manual" }
        ));
        parts.join(" | ")
    } else {
        format!(
            " USB Topology ({}) [{}] ",
            app.device_count_str(),
            if app.auto_refresh { "auto" } else { "manual" }
        )
    };

    let list = List::new(list_items).block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::White)),
    );

    frame.render_widget(list, area);
}

/// Render summary view (all buses).
fn render_summary(frame: &mut Frame, app: &App, area: Rect) {
    let buses = app.topology.buses_sorted();

    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        "Bus Overview",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    for (i, bus) in buses.iter().enumerate() {
        let pool = BandwidthPool::with_usage(bus.speed, bus.periodic_bandwidth_used_bps());
        let is_selected = i == app.selected;

        let style = if is_selected {
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let speed_style = if bus.is_superspeed() {
            Style::default().fg(Color::Magenta)
        } else {
            Style::default().fg(Color::Green)
        };

        let usage_color = if pool.is_critical() {
            Color::Red
        } else if pool.is_high_usage() {
            Color::Yellow
        } else {
            Color::Green
        };

        // Bus header
        let label = app
            .config
            .bus_label(bus.bus_num)
            .unwrap_or_else(|| format!("Bus {}", bus.bus_num));

        lines.push(Line::from(vec![
            Span::styled(format!("{:<20}", label), style),
            Span::styled(format!("{:>6}", bus.speed.short_name()), speed_style),
        ]));

        // Bandwidth bar
        let bar = bandwidth_bar(pool.periodic_usage_percent(), 30);
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(bar, Style::default().fg(usage_color)),
            Span::styled(
                format!(" {:.1}%", pool.periodic_usage_percent()),
                Style::default().fg(usage_color),
            ),
        ]));

        // Bandwidth numbers
        lines.push(Line::from(vec![
            Span::raw("  Used: "),
            Span::styled(
                format!("{:>12}", format_bandwidth(pool.used_periodic_bps)),
                Style::default().fg(Color::White),
            ),
            Span::raw(" / "),
            Span::styled(
                format_bandwidth(pool.max_periodic_bps),
                Style::default().fg(Color::DarkGray),
            ),
        ]));

        // Device count
        lines.push(Line::from(vec![
            Span::raw("  Devices: "),
            Span::styled(
                format!("{}", bus.device_count()),
                Style::default().fg(Color::White),
            ),
        ]));

        lines.push(Line::from(""));
    }

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Bus Summary (press 't' for tree) ")
                .borders(Borders::ALL),
        )
        .wrap(Wrap { trim: true });

    frame.render_widget(paragraph, area);
}

/// Render details panel.
fn render_details(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines = Vec::new();

    // Show device details if selected
    if let Some(device) = app.get_selected_device() {
        lines.push(Line::from(Span::styled(
            "Device Details",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));

        // Name
        lines.push(Line::from(vec![
            Span::styled("Name: ", Style::default().fg(Color::DarkGray)),
            Span::styled(device.display_name(), Style::default().fg(Color::White)),
        ]));

        // Path
        lines.push(Line::from(vec![
            Span::styled("Path: ", Style::default().fg(Color::DarkGray)),
            Span::raw(&device.path.0),
        ]));

        // VID:PID
        lines.push(Line::from(vec![
            Span::styled("VID:PID: ", Style::default().fg(Color::DarkGray)),
            Span::raw(device.vid_pid()),
        ]));

        // Speed
        lines.push(Line::from(vec![
            Span::styled("Speed: ", Style::default().fg(Color::DarkGray)),
            Span::raw(device.speed.to_string()),
        ]));

        // Manufacturer
        if let Some(mfr) = &device.manufacturer {
            lines.push(Line::from(vec![
                Span::styled("Manufacturer: ", Style::default().fg(Color::DarkGray)),
                Span::raw(mfr),
            ]));
        }

        // Product
        if let Some(prod) = &device.product {
            lines.push(Line::from(vec![
                Span::styled("Product: ", Style::default().fg(Color::DarkGray)),
                Span::raw(prod),
            ]));
        }

        // Serial
        if let Some(serial) = &device.serial {
            lines.push(Line::from(vec![
                Span::styled("Serial: ", Style::default().fg(Color::DarkGray)),
                Span::styled(serial, Style::default().fg(Color::Yellow)),
            ]));
        }

        // USB Version
        lines.push(Line::from(vec![
            Span::styled("USB Version: ", Style::default().fg(Color::DarkGray)),
            Span::raw(&device.usb_version),
        ]));

        // Physical location
        if let Some(loc) = &device.physical_location {
            let loc_str = loc.display();
            if !loc_str.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled("Location: ", Style::default().fg(Color::DarkGray)),
                    Span::raw(loc_str),
                ]));
            }
        }

        // Endpoints
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("Endpoints ({})", device.endpoints.len()),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));

        let periodic = device.periodic_endpoints();
        if !periodic.is_empty() {
            lines.push(Line::from(Span::styled(
                "Periodic (bandwidth-reserving):",
                Style::default().fg(Color::Yellow),
            )));

            for ep in &periodic {
                let bw = ep.bandwidth_bps(device.speed);
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        format!("EP{:02X}", ep.address),
                        Style::default().fg(Color::White),
                    ),
                    Span::raw(format!(
                        " {} {} {}B @ {}",
                        ep.transfer_type, ep.direction, ep.max_packet_size, ep.interval_str
                    )),
                ]));
                lines.push(Line::from(vec![
                    Span::raw("       "),
                    Span::styled(
                        format!("→ {}", format_bandwidth(bw)),
                        Style::default().fg(Color::Green),
                    ),
                ]));
            }

            let total_bw = device.periodic_bandwidth_bps();
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Total: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format_bandwidth(total_bw),
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        } else {
            lines.push(Line::from(Span::styled(
                "No periodic endpoints",
                Style::default().fg(Color::DarkGray),
            )));
        }
    } else if let Some(bus) = app.get_selected_bus() {
        // Show bus details
        lines.push(Line::from(Span::styled(
            "Bus Details",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));

        lines.push(Line::from(vec![
            Span::styled("Bus Number: ", Style::default().fg(Color::DarkGray)),
            Span::raw(format!("{}", bus.bus_num)),
        ]));

        lines.push(Line::from(vec![
            Span::styled("Speed: ", Style::default().fg(Color::DarkGray)),
            Span::raw(bus.speed.to_string()),
        ]));

        lines.push(Line::from(vec![
            Span::styled("USB Version: ", Style::default().fg(Color::DarkGray)),
            Span::raw(&bus.version),
        ]));

        lines.push(Line::from(vec![
            Span::styled("Root Ports: ", Style::default().fg(Color::DarkGray)),
            Span::raw(format!("{}", bus.num_ports)),
        ]));

        lines.push(Line::from(vec![
            Span::styled("Devices: ", Style::default().fg(Color::DarkGray)),
            Span::raw(format!("{}", bus.device_count())),
        ]));

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Bandwidth",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));

        let pool = BandwidthPool::with_usage(bus.speed, bus.periodic_bandwidth_used_bps());
        let usage_color = if pool.is_critical() {
            Color::Red
        } else if pool.is_high_usage() {
            Color::Yellow
        } else {
            Color::Green
        };

        lines.push(Line::from(vec![
            Span::styled("Used: ", Style::default().fg(Color::DarkGray)),
            Span::styled(pool.format_used(), Style::default().fg(usage_color)),
        ]));

        lines.push(Line::from(vec![
            Span::styled("Max:  ", Style::default().fg(Color::DarkGray)),
            Span::raw(pool.format_max()),
        ]));

        lines.push(Line::from(vec![
            Span::styled("Avail: ", Style::default().fg(Color::DarkGray)),
            Span::styled(pool.format_available(), Style::default().fg(Color::Green)),
        ]));

        lines.push(Line::from(""));

        // Bandwidth bar
        let bar = bandwidth_bar(pool.periodic_usage_percent(), 25);
        lines.push(Line::from(vec![
            Span::styled(bar, Style::default().fg(usage_color)),
            Span::styled(
                format!(" {:.1}%", pool.periodic_usage_percent()),
                Style::default().fg(usage_color),
            ),
        ]));
    } else {
        lines.push(Line::from(Span::styled(
            "Select a device or bus",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let paragraph = Paragraph::new(lines)
        .block(Block::default().title(" Details ").borders(Borders::ALL))
        .wrap(Wrap { trim: true });

    frame.render_widget(paragraph, area);
}

/// Render help overlay.
fn render_help(frame: &mut Frame) {
    let area = centered_rect(50, 70, frame.area());

    frame.render_widget(Clear, area);

    let help_text = vec![
        Line::from(Span::styled(
            "usbbw Help",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Navigation",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("  j/↓     Move down"),
        Line::from("  k/↑     Move up"),
        Line::from("  Enter   Expand/collapse"),
        Line::from("  g       Go to top"),
        Line::from("  G       Go to bottom"),
        Line::from("  x       Expand/collapse all"),
        Line::from(""),
        Line::from(Span::styled(
            "Views",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("  t       Tree view"),
        Line::from("  s       Summary view"),
        Line::from("  b       Toggle bandwidth bars"),
        Line::from(""),
        Line::from(Span::styled(
            "Device Labels",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("  e       Edit label for selected device"),
        Line::from("  m       Mark device as seen (clear NEW)"),
        Line::from("  w       Write pending labels to config"),
        Line::from(""),
        Line::from(Span::styled(
            "Actions",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("  r       Refresh topology"),
        Line::from("  a       Toggle auto-refresh"),
        Line::from("  ?       Toggle help"),
        Line::from("  q       Quit"),
    ];

    let paragraph = Paragraph::new(help_text)
        .block(
            Block::default()
                .title(" Help ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .style(Style::default().bg(Color::Black));

    frame.render_widget(paragraph, area);
}

/// Create a centered rect.
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Render contextual footer with keybindings.
fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    let mut spans = Vec::new();

    // Check for status message first
    if let Some(status) = app.status() {
        spans.push(Span::styled(
            status,
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ));
    } else {
        // Navigation keys
        spans.push(Span::styled("j/k", Style::default().fg(Color::Yellow)));
        spans.push(Span::raw(" Nav  "));

        spans.push(Span::styled("Enter", Style::default().fg(Color::Yellow)));
        spans.push(Span::raw(" Expand  "));

        spans.push(Span::styled("x", Style::default().fg(Color::Yellow)));
        spans.push(Span::raw(" All  "));

        // View toggles
        spans.push(Span::styled("t/s", Style::default().fg(Color::Yellow)));
        spans.push(Span::raw(" View  "));

        spans.push(Span::styled("b", Style::default().fg(Color::Yellow)));
        spans.push(Span::raw(" Bars  "));

        // Context-specific: show edit/mark if device selected
        if app.selected_device.is_some() {
            spans.push(Span::styled("e", Style::default().fg(Color::Yellow)));
            spans.push(Span::raw(" Edit  "));

            // Show mark-seen only for new devices
            let items = app.visible_items();
            if let Some(TreeItem::Device { is_new: true, .. }) = items.get(app.selected) {
                spans.push(Span::styled("m", Style::default().fg(Color::Yellow)));
                spans.push(Span::raw(" Mark seen  "));
            }
        }

        // Show write if there are pending labels
        if app.pending_label_count() > 0 {
            spans.push(Span::styled("w", Style::default().fg(Color::LightGreen)));
            spans.push(Span::styled(
                format!(" Write ({})  ", app.pending_label_count()),
                Style::default().fg(Color::LightGreen),
            ));
        }

        spans.push(Span::styled("?", Style::default().fg(Color::Yellow)));
        spans.push(Span::raw(" Help  "));

        spans.push(Span::styled("q", Style::default().fg(Color::Yellow)));
        spans.push(Span::raw(" Quit"));
    }

    let paragraph = Paragraph::new(Line::from(spans)).style(Style::default().bg(Color::DarkGray));

    frame.render_widget(paragraph, area);
}

/// Render with edit overlay.
fn render_with_edit_overlay(frame: &mut Frame, app: &App) {
    // Render the main content (dimmed)
    let outer_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(frame.area());

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(outer_chunks[0]);

    match app.view_mode {
        ViewMode::Tree => render_tree(frame, app, main_chunks[0]),
        ViewMode::Summary => render_summary(frame, app, main_chunks[0]),
    }
    render_details(frame, app, main_chunks[1]);

    // Edit footer
    let footer = Paragraph::new(Line::from(vec![
        Span::styled("Editing label...  ", Style::default().fg(Color::Yellow)),
        Span::styled("Enter", Style::default().fg(Color::Cyan)),
        Span::raw(" Save  "),
        Span::styled("Esc", Style::default().fg(Color::Cyan)),
        Span::raw(" Cancel"),
    ]))
    .style(Style::default().bg(Color::DarkGray));
    frame.render_widget(footer, outer_chunks[1]);

    // Edit popup overlay
    if let Some(edit) = &app.edit_mode {
        let popup_area = centered_rect(50, 20, frame.area());
        frame.render_widget(Clear, popup_area);

        let inner = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Length(3)])
            .margin(1)
            .split(popup_area);

        // Title
        let title = Paragraph::new(format!("Edit label for {}", edit.display_name))
            .style(Style::default().fg(Color::Cyan));
        frame.render_widget(title, inner[0]);

        // Input field
        let input_text = format!("{}█", edit.input);
        let input = Paragraph::new(input_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow))
                    .padding(Padding::horizontal(1)),
            )
            .style(Style::default().fg(Color::White));
        frame.render_widget(input, inner[1]);

        // Outer block
        let block = Block::default()
            .title(" Enter Label ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .style(Style::default().bg(Color::Black));
        frame.render_widget(block, popup_area);
    }
}
