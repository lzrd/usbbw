//! Configuration loading and management.

use crate::model::PhysicalLocation;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Configuration errors.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("Inheritance error: {0}")]
    Inheritance(String),
}

/// Application configuration.
#[derive(Debug, Deserialize, Default)]
pub struct Config {
    /// Global settings.
    #[serde(default)]
    pub settings: Settings,

    /// Controller labels by PCI address.
    #[serde(default)]
    pub controllers: HashMap<String, String>,

    /// Bus labels by bus number (keys are strings like "1", "2", etc.).
    #[serde(default)]
    pub buses: HashMap<String, String>,

    /// Device labels by path (e.g., "3-1.2").
    #[serde(default)]
    pub devices: HashMap<String, String>,

    /// Physical port labels.
    #[serde(default)]
    pub physical_ports: Vec<PhysicalPortLabel>,

    /// Product labels by VID:PID (e.g., "0d28:0204").
    #[serde(default)]
    pub products: HashMap<String, String>,

    /// Mermaid output settings.
    #[serde(default)]
    pub mermaid: MermaidConfig,

    /// Position label mappings for ACPI physical_location values.
    #[serde(default)]
    pub position_labels: PositionLabels,
}

/// Global settings.
#[derive(Debug, Deserialize)]
pub struct Settings {
    /// Refresh interval in milliseconds.
    #[serde(default = "default_refresh_ms")]
    pub refresh_ms: u64,

    /// Color theme: "dark" or "light".
    #[serde(default = "default_theme")]
    pub theme: String,

    /// Show bandwidth in bits per second (true) or bytes (false).
    #[serde(default = "default_use_bits")]
    pub use_bits: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            refresh_ms: default_refresh_ms(),
            theme: default_theme(),
            use_bits: default_use_bits(),
        }
    }
}

fn default_refresh_ms() -> u64 {
    1000
}

fn default_theme() -> String {
    "dark".to_string()
}

fn default_use_bits() -> bool {
    true
}

/// Physical port label configuration.
#[derive(Debug, Deserialize)]
pub struct PhysicalPortLabel {
    /// Panel position to match (optional).
    pub panel: Option<String>,
    /// Horizontal position to match (optional).
    pub horizontal_position: Option<String>,
    /// Vertical position to match (optional).
    pub vertical_position: Option<String>,
    /// Dock status to match (optional).
    pub dock: Option<bool>,
    /// Label to apply.
    pub label: String,
}

/// Mermaid output configuration.
#[derive(Debug, Deserialize, Default)]
pub struct MermaidConfig {
    /// Device paths to hide from diagrams.
    #[serde(default)]
    pub hide_paths: Vec<String>,

    /// Only show devices matching these vendor IDs.
    #[serde(default)]
    pub filter_vendors: Vec<String>,

    /// Collapse hubs with single child.
    #[serde(default)]
    pub collapse_single_child_hubs: bool,
}

/// Position label mappings for ACPI physical_location values.
/// Allows translating ACPI terminology to user-friendly names.
#[derive(Debug, Deserialize, Default, Clone)]
pub struct PositionLabels {
    /// Map panel values (e.g., "left" -> "Left Side")
    #[serde(default)]
    pub panel: HashMap<String, String>,

    /// Map vertical_position values (e.g., "upper" -> "Rear")
    #[serde(default)]
    pub vertical: HashMap<String, String>,

    /// Map horizontal_position values (e.g., "left" -> "Outer")
    #[serde(default)]
    pub horizontal: HashMap<String, String>,
}

impl Config {
    /// Load configuration from default locations.
    /// Search order:
    /// 1. ./usbbw.toml
    /// 2. ~/.config/usbbw/config.toml
    /// 3. /etc/usbbw.toml
    pub fn load() -> Result<Self, ConfigError> {
        let paths = Self::config_paths();

        for path in paths.into_iter().flatten() {
            if path.exists() {
                return Self::load_from_path(&path);
            }
        }

        // No config file found - use defaults
        Ok(Config::default())
    }

    /// Apply default labels from a USB topology.
    ///
    /// This fills in any missing labels based on the detected topology.
    /// Existing labels from config files take priority.
    pub fn apply_defaults_from_topology(&mut self, topology: &UsbTopology) {
        // Add controller labels for any not already configured
        for controller in topology.controllers.values() {
            self.controllers
                .entry(controller.pci_address.clone())
                .or_insert_with(|| "USB Controller".to_string());
        }

        // Add bus labels for any not already configured
        // Keep labels simple - the app shows speed info alongside the label
        for bus in topology.buses.values() {
            self.buses
                .entry(bus.bus_num.to_string())
                .or_insert_with(|| format!("Bus {}", bus.bus_num));
        }

        // Add physical port labels for any not already configured
        let mut seen_locs: std::collections::HashSet<(String, String, String)> =
            std::collections::HashSet::new();
        for label in &self.physical_ports {
            let key = (
                label.panel.clone().unwrap_or_default(),
                label.horizontal_position.clone().unwrap_or_default(),
                label.vertical_position.clone().unwrap_or_default(),
            );
            seen_locs.insert(key);
        }

        for bus in topology.buses.values() {
            for device in bus.devices.values() {
                if let Some(loc) = &device.physical_location {
                    // Skip non-specific locations (center/center is the default when ACPI
                    // doesn't have real location data)
                    let is_default_location =
                        loc.horizontal_position == "center" && loc.vertical_position == "center";
                    if is_default_location {
                        continue;
                    }

                    let key = (
                        loc.panel.clone(),
                        loc.horizontal_position.clone(),
                        loc.vertical_position.clone(),
                    );
                    if !seen_locs.contains(&key)
                        && (!loc.panel.is_empty()
                            || !loc.horizontal_position.is_empty()
                            || !loc.vertical_position.is_empty())
                    {
                        seen_locs.insert(key);

                        // Generate a default label using configured position mappings
                        let mut label_parts = Vec::new();
                        if !loc.panel.is_empty() {
                            let mapped = self
                                .position_labels
                                .panel
                                .get(&loc.panel)
                                .map(|s| s.as_str())
                                .unwrap_or(&loc.panel);
                            label_parts.push(capitalize(mapped));
                        }
                        if !loc.vertical_position.is_empty() {
                            let mapped = self
                                .position_labels
                                .vertical
                                .get(&loc.vertical_position)
                                .map(|s| s.as_str())
                                .unwrap_or(&loc.vertical_position);
                            label_parts.push(capitalize(mapped));
                        }
                        let label = if label_parts.is_empty() {
                            "USB Port".to_string()
                        } else {
                            format!("{} USB Port", label_parts.join(" "))
                        };

                        self.physical_ports.push(PhysicalPortLabel {
                            panel: if loc.panel.is_empty() {
                                None
                            } else {
                                Some(loc.panel.clone())
                            },
                            horizontal_position: if loc.horizontal_position.is_empty() {
                                None
                            } else {
                                Some(loc.horizontal_position.clone())
                            },
                            vertical_position: if loc.vertical_position.is_empty() {
                                None
                            } else {
                                Some(loc.vertical_position.clone())
                            },
                            dock: None,
                            label,
                        });
                    }
                }
            }
        }

        // Add product labels for devices not already configured
        for bus in topology.buses.values() {
            for device in bus.devices.values() {
                if device.is_hub {
                    continue;
                }
                let key = format!("{:04x}:{:04x}", device.vendor_id, device.product_id);
                self.products.entry(key).or_insert_with(|| {
                    device
                        .product
                        .clone()
                        .or_else(|| device.manufacturer.clone())
                        .unwrap_or_else(|| "Unknown Device".to_string())
                });
            }
        }
    }

    /// Load configuration from a specific path, supporting inheritance.
    ///
    /// If the config file contains an `inherit` key (string or array of strings),
    /// the inherited files are loaded first and merged, with the current file's
    /// values taking priority.
    pub fn load_from_path(path: &Path) -> Result<Self, ConfigError> {
        let mut seen = HashSet::new();
        let merged = read_and_flatten_toml(path, &mut seen)?;
        let config: Config = merged.try_into()?;
        Ok(config)
    }

    /// Get list of possible config paths.
    fn config_paths() -> Vec<Option<PathBuf>> {
        vec![
            std::env::current_dir().ok().map(|p| p.join("usbbw.toml")),
            dirs::config_dir().map(|p| p.join("usbbw").join("config.toml")),
            Some(PathBuf::from("/etc/usbbw.toml")),
        ]
    }

    /// Get label for a device, checking all sources in priority order:
    /// 1. Product with serial (VID:PID:iSerial) - specific device
    /// 2. Product without serial (VID:PID) - all devices of this type
    /// 3. Physical location match
    /// 4. Explicit device path label (legacy)
    pub fn device_label(
        &self,
        path: &str,
        vendor_id: u16,
        product_id: u16,
        serial: Option<&str>,
        physical_location: Option<&PhysicalLocation>,
    ) -> Option<String> {
        // Priority 1: Product with serial (VID:PID:iSerial)
        if let Some(serial) = serial {
            let key_with_serial = format!("{:04x}:{:04x}:{}", vendor_id, product_id, serial);
            if let Some(label) = self.products.get(&key_with_serial) {
                return Some(label.clone());
            }
        }

        // Priority 2: Product without serial (VID:PID)
        let product_key = format!("{:04x}:{:04x}", vendor_id, product_id);
        if let Some(label) = self.products.get(&product_key) {
            return Some(label.clone());
        }

        // Priority 3: Physical location match
        if let Some(loc) = physical_location {
            for port_label in &self.physical_ports {
                if Self::matches_physical_location(port_label, loc) {
                    return Some(port_label.label.clone());
                }
            }
        }

        // Priority 4: Explicit device path label (legacy)
        if let Some(label) = self.devices.get(path) {
            return Some(label.clone());
        }

        None
    }

    /// Get label for a controller.
    pub fn controller_label(&self, pci_address: &str) -> Option<String> {
        self.controllers.get(pci_address).cloned()
    }

    /// Get label for a bus.
    pub fn bus_label(&self, bus_num: u8) -> Option<String> {
        self.buses.get(&bus_num.to_string()).cloned()
    }

    /// Check if physical location matches a label config.
    fn matches_physical_location(label: &PhysicalPortLabel, loc: &PhysicalLocation) -> bool {
        let panel_matches = label
            .panel
            .as_ref()
            .map(|p| p == &loc.panel)
            .unwrap_or(true);
        let h_pos_matches = label
            .horizontal_position
            .as_ref()
            .map(|h| h == &loc.horizontal_position)
            .unwrap_or(true);
        let v_pos_matches = label
            .vertical_position
            .as_ref()
            .map(|v| v == &loc.vertical_position)
            .unwrap_or(true);
        let dock_matches = label.dock.map(|d| d == loc.dock).unwrap_or(true);

        panel_matches && h_pos_matches && v_pos_matches && dock_matches
    }

    /// Check if a device path should be hidden in mermaid output.
    pub fn should_hide_path(&self, path: &str) -> bool {
        self.mermaid.hide_paths.contains(&path.to_string())
    }

    /// Check if a vendor should be filtered in mermaid output.
    pub fn should_show_vendor(&self, vendor_id: u16) -> bool {
        if self.mermaid.filter_vendors.is_empty() {
            return true;
        }
        let vendor_str = format!("{:04x}", vendor_id);
        self.mermaid.filter_vendors.contains(&vendor_str)
    }
}

// =============================================================================
// TOML Inheritance Support
// =============================================================================

/// Read a TOML file and flatten any inheritance.
///
/// If the file contains an `inherit` key, the inherited files are loaded first
/// and merged. The `inherit` key can be:
/// - A string: single file to inherit from
/// - An array of strings: multiple files to inherit from (applied in order)
///
/// Paths in `inherit` are relative to the directory containing the config file.
fn read_and_flatten_toml(
    path: &Path,
    seen: &mut HashSet<PathBuf>,
) -> Result<toml::Value, ConfigError> {
    // Prevent circular inheritance
    let canonical = path.canonicalize().map_err(|e| {
        ConfigError::Inheritance(format!("cannot resolve {}: {}", path.display(), e))
    })?;

    if !seen.insert(canonical.clone()) {
        return Err(ConfigError::Inheritance(format!(
            "{} is inherited more than once; circular dependencies are not allowed",
            path.display()
        )));
    }

    // Read and parse the file
    let content = std::fs::read_to_string(path)?;
    let mut doc: toml::Value = toml::from_str(&content)?;

    // Check for inherit key
    let inherit = if let Some(table) = doc.as_table_mut() {
        table.remove("inherit")
    } else {
        None
    };

    let Some(inherit) = inherit else {
        // No inheritance, return as-is
        return Ok(doc);
    };

    // Get the directory containing this config file for resolving relative paths
    let base_dir = path.parent().unwrap_or(Path::new("."));

    // Collect inherited files
    let inherited_paths: Vec<PathBuf> = match inherit {
        toml::Value::String(s) => vec![base_dir.join(&s)],
        toml::Value::Array(arr) => {
            let mut paths = Vec::new();
            for item in arr {
                if let toml::Value::String(s) = item {
                    paths.push(base_dir.join(&s));
                } else {
                    return Err(ConfigError::Inheritance(
                        "inherit array must contain only strings".to_string(),
                    ));
                }
            }
            paths
        }
        _ => {
            return Err(ConfigError::Inheritance(
                "inherit must be a string or array of strings".to_string(),
            ));
        }
    };

    // Load and merge inherited files
    let mut merged: Option<toml::Value> = None;
    for inherited_path in inherited_paths {
        let inherited = read_and_flatten_toml(&inherited_path, seen)?;
        merged = Some(match merged {
            Some(base) => merge_toml_values(base, inherited),
            None => inherited,
        });
    }

    // Merge current file on top of inherited
    let result = match merged {
        Some(base) => merge_toml_values(base, doc),
        None => doc,
    };

    Ok(result)
}

/// Deep-merge two TOML values.
///
/// - Tables are merged recursively (later values override earlier)
/// - Arrays are concatenated
/// - Other values are replaced by the later value
fn merge_toml_values(base: toml::Value, overlay: toml::Value) -> toml::Value {
    match (base, overlay) {
        // Both are tables: merge recursively
        (toml::Value::Table(mut base_table), toml::Value::Table(overlay_table)) => {
            for (key, overlay_value) in overlay_table {
                let merged_value = if let Some(base_value) = base_table.remove(&key) {
                    merge_toml_values(base_value, overlay_value)
                } else {
                    overlay_value
                };
                base_table.insert(key, merged_value);
            }
            toml::Value::Table(base_table)
        }
        // Both are arrays: concatenate
        (toml::Value::Array(mut base_arr), toml::Value::Array(overlay_arr)) => {
            base_arr.extend(overlay_arr);
            toml::Value::Array(base_arr)
        }
        // Different types or non-mergeable: overlay wins
        (_, overlay) => overlay,
    }
}

/// Generate example configuration content.
pub fn example_config() -> &'static str {
    r#"# usbbw configuration file
# Place in ./usbbw.toml, ~/.config/usbbw/config.toml, or /etc/usbbw.toml

[settings]
# Refresh interval in milliseconds (for TUI mode)
refresh_ms = 1000
# Color theme: "dark" or "light"
theme = "dark"
# Show bandwidth in bits per second (true) or bytes (false)
use_bits = true

# Controller labels (by PCI address)
[controllers]
# "0000:c1:00.4" = "AMD Integrated USB"
# "0000:c3:00.0" = "Thunderbolt USB"

# Bus labels (by bus number, use quoted string keys)
[buses]
# "1" = "Internal USB 2.0"
# "2" = "Internal USB 3.1"

# Device path labels
# Format: "bus-port.port.port" = "label"
[devices]
# "3-1" = "Thunderbolt Hub"
# "3-1.2" = "Debug Probe"

# Physical port labels
# Matched by physical_location attributes (ACPI-provided)
# [[physical_ports]]
# panel = "left"
# horizontal_position = "center"
# label = "Left USB-C Port"

# Product labels (by VID:PID)
# Format: "vendor_id:product_id" = "label"
[products]
# "0d28:0204" = "DAPLink Debug Probe"
# "046d:c52b" = "Logitech Unifying Receiver"

# Mermaid diagram output settings
[mermaid]
# Device paths to hide from diagrams
hide_paths = []
# Only show devices matching these vendor IDs (empty = show all)
filter_vendors = []
# Collapse hubs with single child
collapse_single_child_hubs = false
"#
}

use crate::model::UsbTopology;

/// Generate a configuration file based on detected USB topology.
///
/// This scans the current system and generates a TOML config with:
/// - All detected controllers with their PCI addresses
/// - All buses with their USB version
/// - All unique physical_location combinations found
/// - All currently connected devices with their product names
pub fn generate_config(topology: &UsbTopology) -> String {
    let mut output = String::new();

    // Header
    output.push_str("# usbbw configuration file - auto-generated\n");
    output.push_str("# Generated from current system USB topology\n");
    output.push_str("#\n");
    output.push_str("# Place in ./usbbw.toml, ~/.config/usbbw/config.toml, or /etc/usbbw.toml\n");
    output.push_str("# Edit labels below to customize device names in the UI\n\n");

    // Settings section
    output.push_str("[settings]\n");
    output.push_str("refresh_ms = 1000\n");
    output.push_str("theme = \"dark\"\n");
    output.push_str("use_bits = true\n\n");

    // Controllers section
    output.push_str(
        "# =============================================================================\n",
    );
    output.push_str("# USB Controllers (by PCI address)\n");
    output.push_str(
        "# =============================================================================\n",
    );
    output.push_str("# Edit labels to identify controllers by physical location or function\n\n");
    output.push_str("[controllers]\n");

    let mut controllers: Vec<_> = topology.controllers.values().collect();
    controllers.sort_by(|a, b| a.id.0.cmp(&b.id.0));

    for controller in &controllers {
        let usb2_info = controller
            .usb2_bus
            .map(|b| format!("USB 2.0 bus {}", b))
            .unwrap_or_default();
        let usb3_info = controller
            .usb3_bus
            .map(|b| format!("USB 3.x bus {}", b))
            .unwrap_or_default();
        let buses = [usb2_info, usb3_info]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", ");

        // Use simple labels; user can edit to add meaningful names
        output.push_str(&format!(
            "\"{}\" = \"USB Controller\"  # {}\n",
            controller.pci_address, buses
        ));
    }
    output.push('\n');

    // Buses section
    output.push_str(
        "# =============================================================================\n",
    );
    output.push_str("# Bus Labels (by bus number)\n");
    output.push_str(
        "# =============================================================================\n",
    );
    output.push_str(
        "# Odd numbers are typically USB 2.0, even numbers USB 3.x on xHCI controllers\n\n",
    );
    output.push_str("[buses]\n");

    for bus in topology.buses_sorted() {
        let speed_name = if bus.is_superspeed() {
            "USB 3.x"
        } else {
            "USB 2.0"
        };
        // Use simple labels; speed is shown by the app anyway
        output.push_str(&format!(
            "\"{}\" = \"Bus {}\"  # {} {}\n",
            bus.bus_num,
            bus.bus_num,
            speed_name,
            bus.speed.short_name()
        ));
    }
    output.push('\n');

    // Position labels section
    output.push_str(
        "# =============================================================================\n",
    );
    output.push_str("# Position Label Mappings (ACPI -> Display Name)\n");
    output.push_str(
        "# =============================================================================\n",
    );
    output.push_str("# Map ACPI physical_location values to user-friendly names.\n");
    output.push_str("# These are applied when generating auto-labels for physical ports.\n");
    output.push_str("# Example: On Framework laptops, \"upper\" means rear (near hinge).\n\n");
    output.push_str("[position_labels.vertical]\n");
    output.push_str("# upper = \"Rear\"    # Near hinge\n");
    output.push_str("# lower = \"Front\"   # Near front edge\n\n");
    output.push_str("[position_labels.panel]\n");
    output.push_str("# left = \"Left\"\n");
    output.push_str("# right = \"Right\"\n\n");
    output.push_str("[position_labels.horizontal]\n");
    output.push_str("# left = \"Outer\"\n");
    output.push_str("# center = \"Center\"\n\n");

    // Physical ports section
    output.push_str(
        "# =============================================================================\n",
    );
    output.push_str("# Physical Port Labels (from ACPI)\n");
    output.push_str(
        "# =============================================================================\n",
    );
    output.push_str("# These match devices by their physical_location attributes\n");
    output.push_str("# Not all systems expose this information\n\n");

    // Collect unique physical locations (skip center/center which is the non-specific default)
    let mut physical_locs: Vec<(&str, &str, &str)> = Vec::new();
    for bus in topology.buses.values() {
        for device in bus.devices.values() {
            if let Some(loc) = &device.physical_location {
                // Skip non-specific default locations
                if loc.horizontal_position == "center" && loc.vertical_position == "center" {
                    continue;
                }
                let key = (
                    loc.panel.as_str(),
                    loc.horizontal_position.as_str(),
                    loc.vertical_position.as_str(),
                );
                if !physical_locs.contains(&key)
                    && (!loc.panel.is_empty()
                        || !loc.horizontal_position.is_empty()
                        || !loc.vertical_position.is_empty())
                {
                    physical_locs.push(key);
                }
            }
        }
    }
    physical_locs.sort();

    if physical_locs.is_empty() {
        output.push_str("# No specific physical_location attributes found on this system\n");
        output.push_str("# [[physical_ports]]\n");
        output.push_str("# panel = \"left\"\n");
        output.push_str("# vertical_position = \"upper\"\n");
        output.push_str("# label = \"Left Upper USB Port\"\n");
    } else {
        for (panel, h_pos, v_pos) in &physical_locs {
            output.push_str("[[physical_ports]]\n");
            if !panel.is_empty() {
                output.push_str(&format!("panel = \"{}\"\n", panel));
            }
            if !h_pos.is_empty() && *h_pos != "center" {
                output.push_str(&format!("horizontal_position = \"{}\"\n", h_pos));
            }
            if !v_pos.is_empty() {
                output.push_str(&format!("vertical_position = \"{}\"\n", v_pos));
            }
            // Generate a label using raw ACPI values (can be customized via position_labels)
            let mut label_parts = Vec::new();
            if !panel.is_empty() {
                label_parts.push(capitalize(panel));
            }
            if !v_pos.is_empty() {
                label_parts.push(capitalize(v_pos));
            }
            let label = if label_parts.is_empty() {
                "USB Port".to_string()
            } else {
                format!("{} USB Port", label_parts.join(" "))
            };
            output.push_str(&format!("label = \"{}\"\n\n", label));
        }
    }

    // Products section
    output.push_str(
        "# =============================================================================\n",
    );
    output.push_str("# Product Labels (by VID:PID)\n");
    output.push_str(
        "# =============================================================================\n",
    );
    output.push_str("# These apply to any device matching the vendor:product ID\n\n");
    output.push_str("[products]\n");

    // Collect unique VID:PID combinations with their product names
    let mut products: std::collections::HashMap<(u16, u16), String> =
        std::collections::HashMap::new();
    for bus in topology.buses.values() {
        for device in bus.devices.values() {
            // Skip hubs - they're usually not interesting
            if device.is_hub {
                continue;
            }
            let key = (device.vendor_id, device.product_id);
            products.entry(key).or_insert_with(|| {
                device
                    .product
                    .clone()
                    .or_else(|| device.manufacturer.clone())
                    .unwrap_or_else(|| "Unknown Device".to_string())
            });
        }
    }

    let mut products: Vec<_> = products.into_iter().collect();
    products.sort_by(|a, b| a.0.cmp(&b.0));

    for ((vid, pid), name) in &products {
        output.push_str(&format!(
            "\"{:04x}:{:04x}\" = \"{}\"\n",
            vid,
            pid,
            sanitize_toml_string(name)
        ));
    }
    output.push('\n');

    // Devices section (current device paths)
    output.push_str(
        "# =============================================================================\n",
    );
    output.push_str("# Device Path Labels (specific to current topology)\n");
    output.push_str(
        "# =============================================================================\n",
    );
    output.push_str("# These are specific to the current device arrangement\n");
    output.push_str("# They may change if you plug devices into different ports\n\n");
    output.push_str("[devices]\n");

    for bus in topology.buses_sorted() {
        for device in bus.devices_tree_order() {
            let name = device
                .product
                .clone()
                .or_else(|| device.manufacturer.clone())
                .unwrap_or_else(|| {
                    if device.is_hub {
                        "USB Hub".to_string()
                    } else {
                        "Unknown Device".to_string()
                    }
                });

            let icon = if device.is_hub { "Hub" } else { "Dev" };
            output.push_str(&format!(
                "# \"{}\" = \"{}\"  # {} {:04x}:{:04x}\n",
                device.path.0, name, icon, device.vendor_id, device.product_id
            ));
        }
    }
    output.push('\n');

    // Mermaid section
    output.push_str(
        "# =============================================================================\n",
    );
    output.push_str("# Mermaid Diagram Settings\n");
    output.push_str(
        "# =============================================================================\n\n",
    );
    output.push_str("[mermaid]\n");
    output.push_str("hide_paths = []\n");
    output.push_str("filter_vendors = []\n");
    output.push_str("collapse_single_child_hubs = false\n");

    output
}

/// Capitalize first letter of a string.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

/// Sanitize a string for use as a TOML value (escape special chars).
fn sanitize_toml_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}
