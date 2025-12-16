//! TUI application state.

use crate::config::Config;
use crate::model::{BandwidthPool, DevicePath, UsbBus, UsbDevice, UsbTopology, format_bandwidth};
use std::collections::{HashMap, HashSet};

/// View mode for the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    /// Tree view with bandwidth bars.
    #[default]
    Tree,
    /// Summary view of all buses.
    Summary,
}

/// Input mode for editing labels.
#[derive(Debug, Clone)]
pub struct EditState {
    /// Device path being edited.
    pub device_path: String,
    /// Current input buffer.
    pub input: String,
    /// Cursor position in input.
    pub cursor: usize,
}

/// TUI application state.
pub struct App {
    /// Current USB topology.
    pub topology: UsbTopology,
    /// Configuration.
    pub config: Config,
    /// Current view mode.
    pub view_mode: ViewMode,
    /// Currently selected item index.
    pub selected: usize,
    /// Scroll offset for the view.
    pub scroll_offset: usize,
    /// Expanded nodes (collapsed if not in set).
    pub expanded: HashSet<String>,
    /// Show help overlay.
    pub show_help: bool,
    /// Last refresh time.
    pub last_refresh: std::time::Instant,
    /// Whether to auto-refresh.
    pub auto_refresh: bool,
    /// Currently selected device path (for detail view).
    pub selected_device: Option<DevicePath>,
    /// Selected bus number (for summary view).
    pub selected_bus: Option<u8>,

    // --- Discovery tracking ---
    /// Device paths present at app startup.
    pub startup_devices: HashSet<String>,
    /// Device paths discovered during session (in order).
    pub discovery_order: Vec<String>,
    /// Devices marked as "seen" (clears NEW indicator).
    pub seen_devices: HashSet<String>,
    /// Pending label edits (device path -> label).
    pub pending_labels: HashMap<String, String>,

    // --- Display options ---
    /// Show inline bandwidth bars in tree view.
    pub show_bandwidth_bars: bool,

    // --- Edit mode ---
    /// Active edit state (if editing a label).
    pub edit_mode: Option<EditState>,

    // --- Status message ---
    /// Temporary status message to display.
    pub status_message: Option<(String, std::time::Instant)>,
}

impl App {
    /// Create a new app with topology and config.
    pub fn new(topology: UsbTopology, config: Config) -> Self {
        // Default: expand all controllers
        let mut expanded = HashSet::new();
        for controller in topology.controllers.values() {
            expanded.insert(controller.id.0.clone());
        }

        // Capture all device paths present at startup
        let startup_devices: HashSet<String> = topology.all_device_paths().collect();

        Self {
            topology,
            config,
            view_mode: ViewMode::Tree,
            selected: 0,
            scroll_offset: 0,
            expanded,
            show_help: false,
            last_refresh: std::time::Instant::now(),
            auto_refresh: true,
            selected_device: None,
            selected_bus: None,
            startup_devices,
            discovery_order: Vec::new(),
            seen_devices: HashSet::new(),
            pending_labels: HashMap::new(),
            show_bandwidth_bars: false,
            edit_mode: None,
            status_message: None,
        }
    }

    /// Update topology (for refresh).
    pub fn update_topology(&mut self, topology: UsbTopology) {
        // Find newly discovered devices
        for path in topology.all_device_paths() {
            if !self.startup_devices.contains(&path) && !self.discovery_order.contains(&path) {
                self.discovery_order.push(path);
            }
        }
        self.topology = topology;
        self.last_refresh = std::time::Instant::now();
    }

    /// Check if a device is "new" (discovered this session and not yet seen/labeled).
    pub fn is_new_device(&self, path: &str) -> bool {
        !self.startup_devices.contains(path)
            && !self.seen_devices.contains(path)
            && !self.pending_labels.contains_key(path)
            && !self.config.devices.contains_key(path)
    }

    /// Get discovery order number for a device (1-indexed), if new.
    pub fn discovery_number(&self, path: &str) -> Option<usize> {
        if self.is_new_device(path) {
            self.discovery_order
                .iter()
                .position(|p| p == path)
                .map(|i| i + 1)
        } else {
            None
        }
    }

    /// Mark a device as seen (clears NEW indicator without adding a label).
    pub fn mark_seen(&mut self, path: &str) {
        self.seen_devices.insert(path.to_string());
    }

    /// Set a pending label for a device.
    pub fn set_pending_label(&mut self, path: String, label: String) {
        self.pending_labels.insert(path.clone(), label);
        // Also mark as seen
        self.seen_devices.insert(path);
    }

    /// Get count of pending labels.
    pub fn pending_label_count(&self) -> usize {
        self.pending_labels.len()
    }

    /// Get count of new devices.
    pub fn new_device_count(&self) -> usize {
        self.discovery_order
            .iter()
            .filter(|p| self.is_new_device(p))
            .count()
    }

    /// Set a status message (auto-clears after a few seconds).
    pub fn set_status(&mut self, msg: String) {
        self.status_message = Some((msg, std::time::Instant::now()));
    }

    /// Get current status message if not expired.
    pub fn status(&self) -> Option<&str> {
        self.status_message.as_ref().and_then(|(msg, time)| {
            if time.elapsed().as_secs() < 3 {
                Some(msg.as_str())
            } else {
                None
            }
        })
    }

    /// Start editing a label for the selected device.
    pub fn start_edit(&mut self) {
        if let Some(path) = &self.selected_device {
            // Pre-populate with existing pending label or empty
            let existing = self
                .pending_labels
                .get(&path.0)
                .cloned()
                .unwrap_or_default();
            self.edit_mode = Some(EditState {
                device_path: path.0.clone(),
                input: existing.clone(),
                cursor: existing.len(),
            });
        }
    }

    /// Cancel editing.
    pub fn cancel_edit(&mut self) {
        self.edit_mode = None;
    }

    /// Confirm edit and save pending label.
    pub fn confirm_edit(&mut self) {
        if let Some(edit) = self.edit_mode.take()
            && !edit.input.is_empty()
        {
            self.set_pending_label(edit.device_path, edit.input);
        }
    }

    /// Toggle bandwidth bar display.
    pub fn toggle_bandwidth_bars(&mut self) {
        self.show_bandwidth_bars = !self.show_bandwidth_bars;
    }

    /// Toggle expansion of selected item.
    pub fn toggle_expand(&mut self) {
        let items = self.visible_items();
        if let Some(item) = items.get(self.selected) {
            let key = item.key();
            if self.expanded.contains(&key) {
                self.expanded.remove(&key);
            } else {
                self.expanded.insert(key);
            }
        }
    }

    /// Move selection up/down.
    pub fn move_selection(&mut self, delta: i32) {
        let items = self.visible_items();
        let len = items.len();
        if len == 0 {
            return;
        }

        let new_selected = if delta < 0 {
            self.selected.saturating_sub((-delta) as usize)
        } else {
            (self.selected + delta as usize).min(len - 1)
        };

        self.selected = new_selected;
        self.update_selected_device();
    }

    /// Jump to top.
    pub fn goto_top(&mut self) {
        self.selected = 0;
        self.scroll_offset = 0;
        self.update_selected_device();
    }

    /// Jump to bottom.
    pub fn goto_bottom(&mut self) {
        let items = self.visible_items();
        if !items.is_empty() {
            self.selected = items.len() - 1;
        }
        self.update_selected_device();
    }

    /// Update selected device based on current selection.
    fn update_selected_device(&mut self) {
        let items = self.visible_items();
        if let Some(item) = items.get(self.selected) {
            match item {
                TreeItem::Device { path, .. } => {
                    self.selected_device = Some(path.clone());
                    self.selected_bus = path.bus_num();
                }
                TreeItem::Bus { bus_num, .. } => {
                    self.selected_device = None;
                    self.selected_bus = Some(*bus_num);
                }
                TreeItem::Controller { .. } => {
                    self.selected_device = None;
                    self.selected_bus = None;
                }
            }
        }
    }

    /// Toggle view mode.
    pub fn toggle_view_mode(&mut self) {
        self.view_mode = match self.view_mode {
            ViewMode::Tree => ViewMode::Summary,
            ViewMode::Summary => ViewMode::Tree,
        };
        self.selected = 0;
    }

    /// Set view mode.
    pub fn set_view_mode(&mut self, mode: ViewMode) {
        self.view_mode = mode;
        self.selected = 0;
    }

    /// Is item expanded?
    pub fn is_expanded(&self, key: &str) -> bool {
        self.expanded.contains(key)
    }

    /// Get visible tree items based on expansion state.
    pub fn visible_items(&self) -> Vec<TreeItem> {
        match self.view_mode {
            ViewMode::Tree => self.tree_items(),
            ViewMode::Summary => self.summary_items(),
        }
    }

    /// Generate tree items.
    fn tree_items(&self) -> Vec<TreeItem> {
        let mut items = Vec::new();

        for controller in self.topology.controllers_sorted() {
            items.push(TreeItem::Controller {
                id: controller.id.0.clone(),
                label: self
                    .config
                    .controller_label(&controller.pci_address)
                    .unwrap_or_else(|| controller.pci_address.clone()),
                pci_address: controller.pci_address.clone(),
            });

            if self.is_expanded(&controller.id.0) {
                // Add USB 2.0 bus
                if let Some(bus_num) = controller.usb2_bus {
                    self.add_bus_items(&mut items, bus_num, 1);
                }
                // Add USB 3.x bus
                if let Some(bus_num) = controller.usb3_bus {
                    self.add_bus_items(&mut items, bus_num, 1);
                }
            }
        }

        items
    }

    /// Add bus and its devices to items list.
    fn add_bus_items(&self, items: &mut Vec<TreeItem>, bus_num: u8, base_depth: usize) {
        if let Some(bus) = self.topology.buses.get(&bus_num) {
            let pool = BandwidthPool::with_usage(bus.speed, bus.periodic_bandwidth_used_bps());

            items.push(TreeItem::Bus {
                bus_num,
                speed_name: bus.speed.short_name().to_string(),
                usage_percent: pool.periodic_usage_percent(),
                used_bps: pool.used_periodic_bps,
                max_bps: pool.max_periodic_bps,
                depth: base_depth,
                label: self.config.bus_label(bus_num),
            });

            let bus_key = format!("bus{}", bus_num);
            if self.is_expanded(&bus_key) {
                for device in bus.devices_tree_order() {
                    let device_depth = base_depth + 1 + device.path.depth();
                    self.add_device_item(items, device, bus, device_depth);
                }
            }
        }
    }

    /// Add a device item.
    fn add_device_item(
        &self,
        items: &mut Vec<TreeItem>,
        device: &UsbDevice,
        _bus: &UsbBus,
        depth: usize,
    ) {
        // Check for pending label first, then config, then device name
        let label = self
            .pending_labels
            .get(&device.path.0)
            .cloned()
            .or_else(|| {
                self.config.device_label(
                    &device.path.0,
                    device.vendor_id,
                    device.product_id,
                    device.physical_location.as_ref(),
                )
            })
            .unwrap_or_else(|| device.display_name());

        let bandwidth = device.periodic_bandwidth_bps();
        let is_new = self.is_new_device(&device.path.0);
        let discovery_number = self.discovery_number(&device.path.0);

        items.push(TreeItem::Device {
            path: device.path.clone(),
            label,
            is_hub: device.is_hub,
            vid_pid: device.vid_pid(),
            bandwidth_bps: bandwidth,
            speed_name: device.speed.short_name().to_string(),
            depth,
            has_children: !device.children.is_empty(),
            is_new,
            discovery_number,
        });
    }

    /// Generate summary items (one per bus).
    fn summary_items(&self) -> Vec<TreeItem> {
        self.topology
            .buses_sorted()
            .iter()
            .map(|bus| {
                let pool = BandwidthPool::with_usage(bus.speed, bus.periodic_bandwidth_used_bps());
                TreeItem::Bus {
                    bus_num: bus.bus_num,
                    speed_name: bus.speed.short_name().to_string(),
                    usage_percent: pool.periodic_usage_percent(),
                    used_bps: pool.used_periodic_bps,
                    max_bps: pool.max_periodic_bps,
                    depth: 0,
                    label: self.config.bus_label(bus.bus_num),
                }
            })
            .collect()
    }

    /// Get the currently selected device (if any).
    pub fn get_selected_device(&self) -> Option<&UsbDevice> {
        self.selected_device
            .as_ref()
            .and_then(|path| self.topology.get_device(path))
    }

    /// Get the currently selected bus (if any).
    pub fn get_selected_bus(&self) -> Option<&UsbBus> {
        self.selected_bus
            .and_then(|num| self.topology.buses.get(&num))
    }

    /// Get device count string.
    pub fn device_count_str(&self) -> String {
        let total = self.topology.total_device_count();
        let buses = self.topology.buses.len();
        format!("{} devices on {} buses", total, buses)
    }
}

/// Tree item types for rendering.
#[derive(Debug, Clone)]
pub enum TreeItem {
    Controller {
        id: String,
        label: String,
        pci_address: String,
    },
    Bus {
        bus_num: u8,
        speed_name: String,
        usage_percent: f64,
        used_bps: u64,
        max_bps: u64,
        depth: usize,
        label: Option<String>,
    },
    Device {
        path: DevicePath,
        label: String,
        is_hub: bool,
        vid_pid: String,
        bandwidth_bps: u64,
        speed_name: String,
        depth: usize,
        has_children: bool,
        /// Is this a "new" device (discovered this session, not yet seen/labeled)?
        is_new: bool,
        /// Discovery order number (1-indexed) if new.
        discovery_number: Option<usize>,
    },
}

impl TreeItem {
    /// Get unique key for expansion tracking.
    pub fn key(&self) -> String {
        match self {
            TreeItem::Controller { id, .. } => id.clone(),
            TreeItem::Bus { bus_num, .. } => format!("bus{}", bus_num),
            TreeItem::Device { path, .. } => path.0.clone(),
        }
    }

    /// Get depth for indentation.
    pub fn depth(&self) -> usize {
        match self {
            TreeItem::Controller { .. } => 0,
            TreeItem::Bus { depth, .. } => *depth,
            TreeItem::Device { depth, .. } => *depth,
        }
    }

    /// Format as display line.
    pub fn display_line(&self) -> String {
        match self {
            TreeItem::Controller { label, .. } => {
                format!("▶ {}", label)
            }
            TreeItem::Bus {
                bus_num,
                speed_name,
                usage_percent,
                label,
                ..
            } => {
                let name = label.clone().unwrap_or_else(|| format!("Bus {}", bus_num));
                format!("⚡ {} ({}) [{:.1}%]", name, speed_name, usage_percent)
            }
            TreeItem::Device {
                label,
                is_hub,
                bandwidth_bps,
                ..
            } => {
                let icon = if *is_hub { "🔀" } else { "📱" };
                if *bandwidth_bps > 0 {
                    format!("{} {} [{}]", icon, label, format_bandwidth(*bandwidth_bps))
                } else {
                    format!("{} {}", icon, label)
                }
            }
        }
    }
}
