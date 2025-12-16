//! Sysfs parser for USB device information.

use crate::model::{
    ControllerId, DevicePath, Direction, Endpoint, PhysicalLocation, TransferType, UsbBus,
    UsbController, UsbDevice, UsbSpeed, UsbTopology,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

const SYSFS_USB_DEVICES: &str = "/sys/bus/usb/devices";

/// Errors that can occur during sysfs parsing.
#[derive(Debug, Error)]
pub enum SysfsError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error for attribute '{0}': {1}")]
    Parse(String, String),
    #[error("Missing attribute: {0}")]
    MissingAttribute(String),
}

/// Parser for Linux sysfs USB device information.
pub struct SysfsParser {
    base_path: PathBuf,
}

impl Default for SysfsParser {
    fn default() -> Self {
        Self::new()
    }
}

impl SysfsParser {
    /// Create a new parser using the default sysfs path.
    pub fn new() -> Self {
        Self {
            base_path: PathBuf::from(SYSFS_USB_DEVICES),
        }
    }

    /// Create a parser with a custom base path (for testing).
    pub fn with_base_path(base_path: impl AsRef<Path>) -> Self {
        Self {
            base_path: base_path.as_ref().to_path_buf(),
        }
    }

    /// Parse complete USB topology from sysfs.
    pub fn parse_topology(&self) -> Result<UsbTopology, SysfsError> {
        let mut topology = UsbTopology::new();

        // First pass: find all root hubs (usbN)
        for entry in std::fs::read_dir(&self.base_path)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();

            if let Some(bus_num_str) = name.strip_prefix("usb")
                && let Ok(bus_num) = bus_num_str.parse::<u8>()
            {
                match self.parse_bus(bus_num) {
                    Ok(bus) => {
                        // Extract or create controller
                        let controller_id = self.get_controller_id(bus_num)?;

                        let controller = topology
                            .controllers
                            .entry(controller_id.clone())
                            .or_insert_with(|| UsbController {
                                id: controller_id.clone(),
                                pci_address: self.get_pci_address(bus_num).unwrap_or_default(),
                                usb2_bus: None,
                                usb3_bus: None,
                                label: None,
                            });

                        if bus.is_superspeed() {
                            controller.usb3_bus = Some(bus_num);
                        } else {
                            controller.usb2_bus = Some(bus_num);
                        }

                        topology.buses.insert(bus_num, bus);
                    }
                    Err(e) => {
                        eprintln!("Warning: Failed to parse bus {}: {}", bus_num, e);
                    }
                }
            }
        }

        // Second pass: parse all devices
        for entry in std::fs::read_dir(&self.base_path)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();

            // Match device paths like "3-1", "3-1.2", etc. (contain '-', no ':')
            if name.contains('-') && !name.contains(':') {
                match self.parse_device(&name) {
                    Ok(device) => {
                        if let Some(bus_num) = device.path.bus_num()
                            && let Some(bus) = topology.buses.get_mut(&bus_num)
                        {
                            bus.devices.insert(device.path.clone(), device);
                        }
                    }
                    Err(e) => {
                        eprintln!("Warning: Failed to parse device {}: {}", name, e);
                    }
                }
            }
        }

        // Third pass: build parent-child relationships
        for bus in topology.buses.values_mut() {
            let paths: Vec<DevicePath> = bus.devices.keys().cloned().collect();
            for path in paths {
                if let Some(parent_path) = path.parent() {
                    // Check if parent is another device (not root hub)
                    if !parent_path.is_root_hub()
                        && let Some(parent) = bus.devices.get_mut(&parent_path)
                        && !parent.children.contains(&path)
                    {
                        parent.children.push(path.clone());
                    }
                }
            }

            // Sort children for consistent ordering
            for device in bus.devices.values_mut() {
                device.children.sort_by(|a, b| a.0.cmp(&b.0));
            }
        }

        Ok(topology)
    }

    /// Parse a root hub (bus).
    fn parse_bus(&self, bus_num: u8) -> Result<UsbBus, SysfsError> {
        let path = self.base_path.join(format!("usb{}", bus_num));

        let speed = self.read_attr_u32(&path, "speed")?;
        let version = self.read_attr_string(&path, "version").unwrap_or_default();
        let num_ports = self.read_attr_u8(&path, "maxchild").unwrap_or(0);

        Ok(UsbBus {
            bus_num,
            speed: UsbSpeed::from_mbps(speed).unwrap_or(UsbSpeed::Full),
            version: version.trim().to_string(),
            num_ports,
            devices: HashMap::new(),
            controller_id: self.get_controller_id(bus_num)?,
        })
    }

    /// Parse a USB device.
    fn parse_device(&self, name: &str) -> Result<UsbDevice, SysfsError> {
        let path = self.base_path.join(name);

        let speed = self.read_attr_u32(&path, "speed")?;
        let vendor_id = self.read_hex_attr_u16(&path, "idVendor")?;
        let product_id = self.read_hex_attr_u16(&path, "idProduct")?;
        let manufacturer = self
            .read_attr_string(&path, "manufacturer")
            .ok()
            .map(|s| s.trim().to_string());
        let product = self
            .read_attr_string(&path, "product")
            .ok()
            .map(|s| s.trim().to_string());
        let serial = self
            .read_attr_string(&path, "serial")
            .ok()
            .map(|s| s.trim().to_string());
        let device_class = self.read_hex_attr_u8(&path, "bDeviceClass").unwrap_or(0);
        let usb_version = self.read_attr_string(&path, "version").unwrap_or_default();
        let num_interfaces = self.read_attr_u8(&path, "bNumInterfaces").unwrap_or(1);

        // Check if device is configured (bConfigurationValue is set)
        // Empty or 0 means device failed to configure (e.g., bandwidth allocation failed)
        let is_configured = self
            .read_attr_u8(&path, "bConfigurationValue")
            .map(|v| v > 0)
            .unwrap_or(false);

        let is_hub = device_class == 0x09;
        let num_ports = if is_hub {
            self.read_attr_u8(&path, "maxchild").ok()
        } else {
            None
        };

        // Parse physical location if present
        let physical_location = self.parse_physical_location(&path).ok();

        // Parse endpoints from all interfaces (only for configured devices)
        let endpoints = if is_configured {
            self.parse_all_endpoints(&path)?
        } else {
            Vec::new()
        };

        // Parse max power consumption (bMaxPower is like "500mA" or "0mA")
        let max_power_ma = self.parse_max_power(&path).unwrap_or(0);

        Ok(UsbDevice {
            path: DevicePath::new(name),
            speed: UsbSpeed::from_mbps(speed).unwrap_or(UsbSpeed::Full),
            vendor_id,
            product_id,
            manufacturer,
            product,
            serial,
            device_class,
            is_hub,
            num_ports,
            endpoints,
            physical_location,
            children: Vec::new(),
            label: None,
            usb_version: usb_version.trim().to_string(),
            num_interfaces,
            max_power_ma,
            is_configured,
        })
    }

    /// Parse all endpoints from all interfaces of a device.
    fn parse_all_endpoints(&self, device_path: &Path) -> Result<Vec<Endpoint>, SysfsError> {
        let mut endpoints = Vec::new();

        // Find all interface directories (e.g., "3-1.2:1.0")
        let entries = match std::fs::read_dir(device_path) {
            Ok(e) => e,
            Err(_) => return Ok(endpoints),
        };

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();

            // Interface directories contain ':'
            if name.contains(':') && entry.path().is_dir() {
                // Find endpoint directories within interface
                if let Ok(iface_entries) = std::fs::read_dir(entry.path()) {
                    for ep_entry in iface_entries.flatten() {
                        let ep_name = ep_entry.file_name().to_string_lossy().to_string();

                        // Match ep_XX but not ep_00 (control endpoint)
                        if ep_name.starts_with("ep_")
                            && ep_name != "ep_00"
                            && let Ok(ep) = self.parse_endpoint(&ep_entry.path())
                        {
                            endpoints.push(ep);
                        }
                    }
                }
            }
        }

        Ok(endpoints)
    }

    /// Parse a single endpoint.
    fn parse_endpoint(&self, path: &Path) -> Result<Endpoint, SysfsError> {
        let type_str = self.read_attr_string(path, "type")?;
        let transfer_type = TransferType::from_sysfs(&type_str)
            .ok_or_else(|| SysfsError::Parse("type".to_string(), type_str.clone()))?;

        let direction_str = self.read_attr_string(path, "direction")?;
        let direction = Direction::from_sysfs(&direction_str)
            .ok_or_else(|| SysfsError::Parse("direction".to_string(), direction_str.clone()))?;

        // Address from bEndpointAddress (hex)
        let address = self.read_hex_attr_u8(path, "bEndpointAddress")?;

        // Interval and packet size (hex)
        let b_interval = self.read_hex_attr_u8(path, "bInterval").unwrap_or(0);
        let max_packet_size = self.read_hex_attr_u16(path, "wMaxPacketSize").unwrap_or(0);
        let interval_str = self
            .read_attr_string(path, "interval")
            .unwrap_or_else(|_| "?".to_string());

        Ok(Endpoint {
            address,
            transfer_type,
            direction,
            max_packet_size,
            b_interval,
            interval_str: interval_str.trim().to_string(),
        })
    }

    /// Parse physical location attributes.
    fn parse_physical_location(&self, device_path: &Path) -> Result<PhysicalLocation, SysfsError> {
        let loc_path = device_path.join("physical_location");

        if !loc_path.exists() {
            return Err(SysfsError::MissingAttribute(
                "physical_location".to_string(),
            ));
        }

        Ok(PhysicalLocation {
            dock: self
                .read_attr_string(&loc_path, "dock")
                .map(|s| s.trim() == "yes")
                .unwrap_or(false),
            panel: self
                .read_attr_string(&loc_path, "panel")
                .unwrap_or_default()
                .trim()
                .to_string(),
            horizontal_position: self
                .read_attr_string(&loc_path, "horizontal_position")
                .unwrap_or_default()
                .trim()
                .to_string(),
            vertical_position: self
                .read_attr_string(&loc_path, "vertical_position")
                .unwrap_or_default()
                .trim()
                .to_string(),
            lid: self
                .read_attr_string(&loc_path, "lid")
                .map(|s| s.trim() == "yes")
                .unwrap_or(false),
        })
    }

    /// Get controller ID from bus number by reading symlink.
    fn get_controller_id(&self, bus_num: u8) -> Result<ControllerId, SysfsError> {
        let link = self.base_path.join(format!("usb{}", bus_num));

        // Try to read symlink to get PCI path
        if let Ok(target) = std::fs::read_link(&link) {
            let path_str = target.to_string_lossy();

            // Extract PCI address from path like:
            // ../../../devices/pci0000:00/0000:00:08.1/0000:c1:00.4/usb1
            // We want the last PCI address before usbN
            let components: Vec<&str> = path_str.split('/').collect();
            for (i, component) in components.iter().enumerate() {
                if component.starts_with("usb") {
                    // Use the component before this one if it looks like a PCI address
                    if i > 0 {
                        let prev = components[i - 1];
                        if prev.len() >= 7 && prev.contains(':') && prev.contains('.') {
                            return Ok(ControllerId(prev.to_string()));
                        }
                    }
                }
            }
        }

        // Fallback: use bus number
        Ok(ControllerId(format!("bus{}", bus_num)))
    }

    /// Get PCI address for a bus.
    fn get_pci_address(&self, bus_num: u8) -> Option<String> {
        self.get_controller_id(bus_num).ok().map(|id| id.0)
    }

    // Helper methods for reading sysfs attributes

    fn read_attr_string(&self, path: &Path, attr: &str) -> Result<String, SysfsError> {
        let content = std::fs::read_to_string(path.join(attr))?;
        Ok(content)
    }

    fn read_attr_u8(&self, path: &Path, attr: &str) -> Result<u8, SysfsError> {
        let content = std::fs::read_to_string(path.join(attr))?;
        content
            .trim()
            .parse()
            .map_err(|e| SysfsError::Parse(attr.to_string(), format!("{}", e)))
    }

    fn read_attr_u32(&self, path: &Path, attr: &str) -> Result<u32, SysfsError> {
        let content = std::fs::read_to_string(path.join(attr))?;
        content
            .trim()
            .parse()
            .map_err(|e| SysfsError::Parse(attr.to_string(), format!("{}", e)))
    }

    fn read_hex_attr_u8(&self, path: &Path, attr: &str) -> Result<u8, SysfsError> {
        let content = std::fs::read_to_string(path.join(attr))?;
        u8::from_str_radix(content.trim(), 16)
            .map_err(|e| SysfsError::Parse(attr.to_string(), format!("{}", e)))
    }

    fn read_hex_attr_u16(&self, path: &Path, attr: &str) -> Result<u16, SysfsError> {
        let content = std::fs::read_to_string(path.join(attr))?;
        u16::from_str_radix(content.trim(), 16)
            .map_err(|e| SysfsError::Parse(attr.to_string(), format!("{}", e)))
    }

    /// Parse bMaxPower attribute (format: "500mA" or "0mA").
    fn parse_max_power(&self, device_path: &Path) -> Result<u16, SysfsError> {
        let content = std::fs::read_to_string(device_path.join("bMaxPower"))?;
        let trimmed = content.trim();

        // Parse "500mA" format - strip the "mA" suffix
        let num_str = trimmed.trim_end_matches("mA");
        num_str
            .parse::<u16>()
            .map_err(|e| SysfsError::Parse("bMaxPower".to_string(), format!("{}", e)))
    }
}
