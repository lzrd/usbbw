//! USB topology data structures.

use super::endpoint::Endpoint;
use super::speed::UsbSpeed;
use std::collections::HashMap;

/// Unique device identifier: bus-port.port.port...
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DevicePath(pub String);

impl DevicePath {
    /// Create a new device path.
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
    }

    /// Get parent device path.
    /// Examples:
    /// - "3-1.2.3" -> "3-1.2"
    /// - "3-1.2" -> "3-1"
    /// - "3-1" -> "usb3" (root hub)
    pub fn parent(&self) -> Option<DevicePath> {
        if let Some(pos) = self.0.rfind('.') {
            Some(DevicePath(self.0[..pos].to_string()))
        } else {
            self.0
                .rfind('-')
                .map(|pos| DevicePath(format!("usb{}", &self.0[..pos])))
        }
    }

    /// Get bus number from path.
    pub fn bus_num(&self) -> Option<u8> {
        self.0
            .split('-')
            .next()
            .or_else(|| self.0.strip_prefix("usb"))
            .and_then(|s| s.parse().ok())
    }

    /// Port path within bus (e.g., "3-1.2.3" -> "1.2.3").
    pub fn port_path(&self) -> Option<&str> {
        self.0.split('-').nth(1)
    }

    /// Depth in the USB tree (0 = direct child of root hub).
    pub fn depth(&self) -> usize {
        self.port_path()
            .map(|p| p.matches('.').count())
            .unwrap_or(0)
    }

    /// Check if this is a root hub path (e.g., "usb3").
    pub fn is_root_hub(&self) -> bool {
        self.0.starts_with("usb")
    }
}

impl std::fmt::Display for DevicePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Physical location attributes (ACPI-provided on some systems).
#[derive(Debug, Clone, Default)]
pub struct PhysicalLocation {
    /// Is this in a dock?
    pub dock: bool,
    /// Panel position: "left", "right", "back", "front", "top", "bottom".
    pub panel: String,
    /// Horizontal position: "left", "center", "right".
    pub horizontal_position: String,
    /// Vertical position: "upper", "center", "lower".
    pub vertical_position: String,
    /// Is this on the lid?
    pub lid: bool,
}

impl PhysicalLocation {
    /// Format as a human-readable string.
    pub fn display(&self) -> String {
        let mut parts = Vec::new();

        if !self.panel.is_empty() && self.panel != "unknown" {
            parts.push(self.panel.clone());
        }
        if !self.vertical_position.is_empty() && self.vertical_position != "unknown" {
            parts.push(self.vertical_position.clone());
        }
        if !self.horizontal_position.is_empty() && self.horizontal_position != "unknown" {
            parts.push(self.horizontal_position.clone());
        }

        if parts.is_empty() {
            String::new()
        } else {
            parts.join(" ")
        }
    }
}

/// A USB device (includes hubs).
#[derive(Debug, Clone)]
pub struct UsbDevice {
    /// Sysfs path identifier (e.g., "3-1.2").
    pub path: DevicePath,
    /// USB speed of this device.
    pub speed: UsbSpeed,
    /// Vendor ID.
    pub vendor_id: u16,
    /// Product ID.
    pub product_id: u16,
    /// Manufacturer string.
    pub manufacturer: Option<String>,
    /// Product string.
    pub product: Option<String>,
    /// Serial number.
    pub serial: Option<String>,
    /// Device class.
    pub device_class: u8,
    /// Is this a hub? (bDeviceClass == 0x09).
    pub is_hub: bool,
    /// Number of ports (if hub).
    pub num_ports: Option<u8>,
    /// All endpoints across all interfaces.
    pub endpoints: Vec<Endpoint>,
    /// Physical location info (on supported systems).
    pub physical_location: Option<PhysicalLocation>,
    /// Children device paths (for hubs).
    pub children: Vec<DevicePath>,
    /// User-defined label from config.
    pub label: Option<String>,
    /// USB version string (e.g., "2.00").
    pub usb_version: String,
    /// Number of interfaces.
    pub num_interfaces: u8,
    /// Maximum power consumption in milliamps (from bMaxPower).
    pub max_power_ma: u16,
    /// Is device configured? False if bandwidth allocation failed.
    pub is_configured: bool,
}

impl UsbDevice {
    /// Get display name (label > product > manufacturer > VID:PID).
    pub fn display_name(&self) -> String {
        self.label
            .clone()
            .or_else(|| self.product.clone())
            .or_else(|| self.manufacturer.clone())
            .unwrap_or_else(|| format!("{:04x}:{:04x}", self.vendor_id, self.product_id))
    }

    /// Calculate total periodic bandwidth reserved by this device.
    pub fn periodic_bandwidth_bps(&self) -> u64 {
        self.endpoints
            .iter()
            .filter(|ep| ep.transfer_type.reserves_bandwidth())
            .map(|ep| ep.bandwidth_bps(self.speed))
            .sum()
    }

    /// Get periodic endpoints.
    pub fn periodic_endpoints(&self) -> Vec<&Endpoint> {
        self.endpoints
            .iter()
            .filter(|ep| ep.transfer_type.reserves_bandwidth())
            .collect()
    }

    /// Format VID:PID as string.
    pub fn vid_pid(&self) -> String {
        format!("{:04x}:{:04x}", self.vendor_id, self.product_id)
    }

    /// Config key for label lookup (VID:PID:iSerial or VID:PID if no serial).
    pub fn config_key(&self) -> String {
        match &self.serial {
            Some(serial) if !serial.is_empty() => {
                format!("{:04x}:{:04x}:{}", self.vendor_id, self.product_id, serial)
            }
            _ => self.vid_pid(),
        }
    }
}

/// Controller identifier (derived from PCI path or bus number).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ControllerId(pub String);

impl std::fmt::Display for ControllerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Controller type (USB, USB4/Thunderbolt, etc.)
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ControllerType {
    #[default]
    Usb,
    /// USB4/Thunderbolt controller
    Usb4,
}

impl std::fmt::Display for ControllerType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ControllerType::Usb => write!(f, "USB"),
            ControllerType::Usb4 => write!(f, "USB4/TB"),
        }
    }
}

/// An xHCI controller with paired USB 2.0 and USB 3.x buses.
#[derive(Debug, Clone)]
pub struct UsbController {
    /// Controller identifier.
    pub id: ControllerId,
    /// PCI address (e.g., "0000:c1:00.4").
    pub pci_address: String,
    /// USB 2.0 bus number (if present).
    pub usb2_bus: Option<u8>,
    /// USB 3.x bus number (if present).
    pub usb3_bus: Option<u8>,
    /// User-defined label.
    pub label: Option<String>,
    /// Controller type (USB or USB4/Thunderbolt).
    pub controller_type: ControllerType,
}

impl UsbController {
    /// Get display name (label > PCI address).
    pub fn display_name(&self) -> String {
        self.label
            .clone()
            .unwrap_or_else(|| self.pci_address.clone())
    }
}

/// A USB bus (root hub).
#[derive(Debug, Clone)]
pub struct UsbBus {
    /// Bus number (1-based).
    pub bus_num: u8,
    /// Speed capability.
    pub speed: UsbSpeed,
    /// USB version string (e.g., "2.00", "3.10").
    pub version: String,
    /// Number of root ports.
    pub num_ports: u8,
    /// Devices on this bus (by path).
    pub devices: HashMap<DevicePath, UsbDevice>,
    /// Controller this bus belongs to.
    pub controller_id: ControllerId,
}

impl UsbBus {
    /// Calculate total periodic bandwidth used on this bus.
    pub fn periodic_bandwidth_used_bps(&self) -> u64 {
        self.devices
            .values()
            .map(|d| d.periodic_bandwidth_bps())
            .sum()
    }

    /// Maximum periodic bandwidth for this bus.
    pub fn max_periodic_bandwidth_bps(&self) -> u64 {
        self.speed.max_periodic_bandwidth_bps()
    }

    /// Periodic bandwidth usage as a percentage.
    pub fn periodic_usage_percent(&self) -> f64 {
        let max = self.max_periodic_bandwidth_bps();
        if max == 0 {
            return 0.0;
        }
        (self.periodic_bandwidth_used_bps() as f64 / max as f64) * 100.0
    }

    /// Is this a SuperSpeed (USB 3.x) bus?
    pub fn is_superspeed(&self) -> bool {
        self.speed.is_superspeed()
    }

    /// Get devices in tree order (depth-first from root ports).
    pub fn devices_tree_order(&self) -> Vec<&UsbDevice> {
        let mut result = Vec::new();

        // Find root-level devices (direct children of root hub)
        let mut root_devices: Vec<_> = self
            .devices
            .values()
            .filter(|d| d.path.depth() == 0)
            .collect();

        // Sort by port number for consistent ordering
        root_devices.sort_by(|a, b| a.path.0.cmp(&b.path.0));

        for device in root_devices {
            self.collect_devices_recursive(device, &mut result);
        }

        result
    }

    fn collect_devices_recursive<'a>(
        &'a self,
        device: &'a UsbDevice,
        result: &mut Vec<&'a UsbDevice>,
    ) {
        result.push(device);
        for child_path in &device.children {
            if let Some(child) = self.devices.get(child_path) {
                self.collect_devices_recursive(child, result);
            }
        }
    }

    /// Get device count.
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    /// Calculate total configured power consumption on this bus (in mA).
    pub fn total_power_ma(&self) -> u32 {
        self.devices.values().map(|d| d.max_power_ma as u32).sum()
    }
}

/// Complete USB topology of the system.
#[derive(Debug, Default)]
pub struct UsbTopology {
    /// All controllers.
    pub controllers: HashMap<ControllerId, UsbController>,
    /// All buses.
    pub buses: HashMap<u8, UsbBus>,
}

impl UsbTopology {
    /// Create a new empty topology.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get all buses sorted by number.
    pub fn buses_sorted(&self) -> Vec<&UsbBus> {
        let mut buses: Vec<_> = self.buses.values().collect();
        buses.sort_by_key(|b| b.bus_num);
        buses
    }

    /// Get all controllers sorted by ID.
    pub fn controllers_sorted(&self) -> Vec<&UsbController> {
        let mut controllers: Vec<_> = self.controllers.values().collect();
        controllers.sort_by(|a, b| a.id.0.cmp(&b.id.0));
        controllers
    }

    /// Get total device count across all buses.
    pub fn total_device_count(&self) -> usize {
        self.buses.values().map(|b| b.device_count()).sum()
    }

    /// Get a device by its path, searching all buses.
    pub fn get_device(&self, path: &DevicePath) -> Option<&UsbDevice> {
        if let Some(bus_num) = path.bus_num()
            && let Some(bus) = self.buses.get(&bus_num)
        {
            return bus.devices.get(path);
        }
        None
    }

    /// Get all device paths across all buses.
    pub fn all_device_paths(&self) -> impl Iterator<Item = String> + '_ {
        self.buses
            .values()
            .flat_map(|bus| bus.devices.keys().map(|p| p.0.clone()))
    }

    /// Get the paired bus number for a given bus (USB 2.0 <-> USB 3.x pairing).
    /// Returns None if no pairing exists.
    pub fn get_paired_bus(&self, bus_num: u8) -> Option<u8> {
        for controller in self.controllers.values() {
            if controller.usb2_bus == Some(bus_num) {
                return controller.usb3_bus;
            }
            if controller.usb3_bus == Some(bus_num) {
                return controller.usb2_bus;
            }
        }
        None
    }

    /// Get controller for a given bus number.
    pub fn get_controller_for_bus(&self, bus_num: u8) -> Option<&UsbController> {
        self.controllers
            .values()
            .find(|c| c.usb2_bus == Some(bus_num) || c.usb3_bus == Some(bus_num))
    }
}

/// Format bandwidth as human-readable string.
pub fn format_bandwidth(bps: u64) -> String {
    if bps >= 1_000_000_000 {
        format!("{:.2} Gbps", bps as f64 / 1_000_000_000.0)
    } else if bps >= 1_000_000 {
        format!("{:.2} Mbps", bps as f64 / 1_000_000.0)
    } else if bps >= 1_000 {
        format!("{:.2} Kbps", bps as f64 / 1_000.0)
    } else {
        format!("{} bps", bps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_path_parent() {
        let path = DevicePath::new("3-1.2.3");
        assert_eq!(path.parent(), Some(DevicePath::new("3-1.2")));

        let path2 = DevicePath::new("3-1");
        assert_eq!(path2.parent(), Some(DevicePath::new("usb3")));
    }

    #[test]
    fn test_device_path_depth() {
        assert_eq!(DevicePath::new("3-1").depth(), 0);
        assert_eq!(DevicePath::new("3-1.2").depth(), 1);
        assert_eq!(DevicePath::new("3-1.2.3").depth(), 2);
    }

    #[test]
    fn test_format_bandwidth() {
        assert_eq!(format_bandwidth(500), "500 bps");
        assert_eq!(format_bandwidth(64_000), "64.00 Kbps");
        assert_eq!(format_bandwidth(480_000_000), "480.00 Mbps");
        assert_eq!(format_bandwidth(5_000_000_000), "5.00 Gbps");
    }
}
