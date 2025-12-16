//! Bandwidth pool calculations and formatting.

use super::speed::UsbSpeed;

/// Bandwidth pool for a bus.
#[derive(Debug, Clone)]
pub struct BandwidthPool {
    /// Maximum available for periodic transfers (bps).
    pub max_periodic_bps: u64,
    /// Currently reserved by periodic endpoints (bps).
    pub used_periodic_bps: u64,
    /// Raw bus bandwidth (bps).
    pub raw_bandwidth_bps: u64,
    /// Bus speed.
    pub speed: UsbSpeed,
}

impl BandwidthPool {
    /// Create a new bandwidth pool for a given speed.
    pub fn new(speed: UsbSpeed) -> Self {
        Self {
            max_periodic_bps: speed.max_periodic_bandwidth_bps(),
            used_periodic_bps: 0,
            raw_bandwidth_bps: speed.raw_bandwidth_bps(),
            speed,
        }
    }

    /// Create with known usage.
    pub fn with_usage(speed: UsbSpeed, used_bps: u64) -> Self {
        Self {
            max_periodic_bps: speed.max_periodic_bandwidth_bps(),
            used_periodic_bps: used_bps,
            raw_bandwidth_bps: speed.raw_bandwidth_bps(),
            speed,
        }
    }

    /// Percentage of periodic bandwidth used (0.0 - 100.0).
    pub fn periodic_usage_percent(&self) -> f64 {
        if self.max_periodic_bps == 0 {
            return 0.0;
        }
        (self.used_periodic_bps as f64 / self.max_periodic_bps as f64) * 100.0
    }

    /// Available periodic bandwidth.
    pub fn available_periodic_bps(&self) -> u64 {
        self.max_periodic_bps.saturating_sub(self.used_periodic_bps)
    }

    /// Check if bandwidth pool is near capacity (>80%).
    pub fn is_high_usage(&self) -> bool {
        self.periodic_usage_percent() > 80.0
    }

    /// Check if bandwidth pool is critical (>95%).
    pub fn is_critical(&self) -> bool {
        self.periodic_usage_percent() > 95.0
    }

    /// Add usage to the pool.
    pub fn add_usage(&mut self, bps: u64) {
        self.used_periodic_bps = self.used_periodic_bps.saturating_add(bps);
    }

    /// Format used bandwidth as string.
    pub fn format_used(&self) -> String {
        format_bps(self.used_periodic_bps)
    }

    /// Format max bandwidth as string.
    pub fn format_max(&self) -> String {
        format_bps(self.max_periodic_bps)
    }

    /// Format available bandwidth as string.
    pub fn format_available(&self) -> String {
        format_bps(self.available_periodic_bps())
    }
}

/// Format bits per second as human-readable string.
pub fn format_bps(bps: u64) -> String {
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

/// Format bytes as human-readable string.
pub fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.2} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.2} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.2} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

/// Generate an ASCII bar for bandwidth usage.
pub fn bandwidth_bar(percent: f64, width: usize) -> String {
    let filled = ((percent / 100.0) * width as f64).round() as usize;
    let filled = filled.min(width);
    let empty = width.saturating_sub(filled);

    format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
}

/// Generate a simple ASCII bar without brackets.
pub fn simple_bar(percent: f64, width: usize) -> String {
    let filled = ((percent / 100.0) * width as f64).round() as usize;
    let filled = filled.min(width);
    let empty = width.saturating_sub(filled);

    format!("{}{}", "▓".repeat(filled), "░".repeat(empty))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bandwidth_pool() {
        let mut pool = BandwidthPool::new(UsbSpeed::High);
        assert_eq!(pool.max_periodic_bps, 384_000_000);
        assert_eq!(pool.periodic_usage_percent(), 0.0);

        pool.add_usage(38_400_000); // 10%
        assert!((pool.periodic_usage_percent() - 10.0).abs() < 0.01);
        assert!(!pool.is_high_usage());

        pool.add_usage(307_200_000); // +80% = 90%
        assert!(pool.is_high_usage());
        assert!(!pool.is_critical());
    }

    #[test]
    fn test_format_bps() {
        assert_eq!(format_bps(500), "500 bps");
        assert_eq!(format_bps(1500), "1.50 Kbps");
        assert_eq!(format_bps(1_500_000), "1.50 Mbps");
        assert_eq!(format_bps(1_500_000_000), "1.50 Gbps");
    }

    #[test]
    fn test_bandwidth_bar() {
        assert_eq!(bandwidth_bar(0.0, 10), "[░░░░░░░░░░]");
        assert_eq!(bandwidth_bar(50.0, 10), "[█████░░░░░]");
        assert_eq!(bandwidth_bar(100.0, 10), "[██████████]");
    }
}
