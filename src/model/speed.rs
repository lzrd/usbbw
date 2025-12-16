//! USB speed enumeration with bandwidth characteristics.

use std::fmt;

/// USB speed variants with bandwidth characteristics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UsbSpeed {
    /// USB 1.0 Low Speed - 1.5 Mbps
    Low,
    /// USB 1.1 Full Speed - 12 Mbps
    Full,
    /// USB 2.0 High Speed - 480 Mbps
    High,
    /// USB 3.0/3.1 Gen 1 SuperSpeed - 5 Gbps
    Super,
    /// USB 3.1 Gen 2 SuperSpeed+ - 10 Gbps
    SuperPlus,
    /// USB 3.2 Gen 2x2 SuperSpeed+ - 20 Gbps
    SuperPlus2,
}

impl UsbSpeed {
    /// Parse from sysfs 'speed' attribute (value in Mbps).
    pub fn from_mbps(mbps: u32) -> Option<Self> {
        match mbps {
            1 | 2 => Some(Self::Low), // 1.5 Mbps rounds to 1 or 2
            12 => Some(Self::Full),
            480 => Some(Self::High),
            5000 => Some(Self::Super),
            10000 => Some(Self::SuperPlus),
            20000 => Some(Self::SuperPlus2),
            _ => None,
        }
    }

    /// Raw bandwidth in bits per second.
    pub fn raw_bandwidth_bps(&self) -> u64 {
        match self {
            Self::Low => 1_500_000,
            Self::Full => 12_000_000,
            Self::High => 480_000_000,
            Self::Super => 5_000_000_000,
            Self::SuperPlus => 10_000_000_000,
            Self::SuperPlus2 => 20_000_000_000,
        }
    }

    /// Maximum periodic bandwidth (80% for USB 2.0 and below per spec).
    /// USB spec limits periodic (interrupt + isochronous) transfers to
    /// 90% of full-speed frames and 80% of high-speed microframes.
    pub fn max_periodic_bandwidth_bps(&self) -> u64 {
        match self {
            Self::Low | Self::Full => {
                // Full/Low speed: 90% of bandwidth for periodic transfers
                self.raw_bandwidth_bps() * 90 / 100
            }
            Self::High => {
                // High speed: 80% of bandwidth for periodic transfers
                self.raw_bandwidth_bps() * 80 / 100
            }
            Self::Super | Self::SuperPlus | Self::SuperPlus2 => {
                // USB 3.x: similar model, ~80% effective limit
                self.raw_bandwidth_bps() * 80 / 100
            }
        }
    }

    /// Frame/microframe period in microseconds.
    /// - Low/Full speed: 1ms (1000µs) frames
    /// - High speed and above: 125µs microframes
    pub fn frame_period_us(&self) -> u32 {
        match self {
            Self::Low | Self::Full => 1000,
            _ => 125,
        }
    }

    /// Returns true if this is a USB 3.x SuperSpeed variant.
    pub fn is_superspeed(&self) -> bool {
        matches!(self, Self::Super | Self::SuperPlus | Self::SuperPlus2)
    }

    /// Short display name for TUI.
    pub fn short_name(&self) -> &'static str {
        match self {
            Self::Low => "1.5M",
            Self::Full => "12M",
            Self::High => "480M",
            Self::Super => "5G",
            Self::SuperPlus => "10G",
            Self::SuperPlus2 => "20G",
        }
    }
}

impl fmt::Display for UsbSpeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Low => "Low Speed (1.5 Mbps)",
            Self::Full => "Full Speed (12 Mbps)",
            Self::High => "High Speed (480 Mbps)",
            Self::Super => "SuperSpeed (5 Gbps)",
            Self::SuperPlus => "SuperSpeed+ (10 Gbps)",
            Self::SuperPlus2 => "SuperSpeed+ 2x2 (20 Gbps)",
        };
        write!(f, "{}", name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_mbps() {
        assert_eq!(UsbSpeed::from_mbps(480), Some(UsbSpeed::High));
        assert_eq!(UsbSpeed::from_mbps(5000), Some(UsbSpeed::Super));
        assert_eq!(UsbSpeed::from_mbps(10000), Some(UsbSpeed::SuperPlus));
        assert_eq!(UsbSpeed::from_mbps(999), None);
    }

    #[test]
    fn test_bandwidth() {
        assert_eq!(UsbSpeed::High.raw_bandwidth_bps(), 480_000_000);
        assert_eq!(UsbSpeed::High.max_periodic_bandwidth_bps(), 384_000_000);
    }

    #[test]
    fn test_is_superspeed() {
        assert!(!UsbSpeed::High.is_superspeed());
        assert!(UsbSpeed::Super.is_superspeed());
        assert!(UsbSpeed::SuperPlus.is_superspeed());
    }
}
