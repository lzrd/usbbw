//! USB endpoint model with bandwidth calculation.

use super::speed::UsbSpeed;
use std::fmt;

/// USB transfer types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferType {
    Control,
    Bulk,
    Interrupt,
    Isochronous,
}

impl TransferType {
    /// Parse from sysfs 'type' attribute string.
    pub fn from_sysfs(s: &str) -> Option<Self> {
        match s.trim() {
            "Control" => Some(Self::Control),
            "Bulk" => Some(Self::Bulk),
            "Interrupt" => Some(Self::Interrupt),
            "Isoc" | "Isochronous" => Some(Self::Isochronous),
            _ => None,
        }
    }

    /// Returns true if this transfer type reserves bandwidth.
    /// Only Interrupt and Isochronous endpoints reserve bandwidth.
    pub fn reserves_bandwidth(&self) -> bool {
        matches!(self, Self::Interrupt | Self::Isochronous)
    }
}

impl fmt::Display for TransferType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Control => "Control",
            Self::Bulk => "Bulk",
            Self::Interrupt => "Interrupt",
            Self::Isochronous => "Isochronous",
        };
        write!(f, "{}", name)
    }
}

/// Endpoint direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    In,
    Out,
}

impl Direction {
    /// Parse from sysfs 'direction' attribute.
    pub fn from_sysfs(s: &str) -> Option<Self> {
        match s.trim() {
            "in" => Some(Self::In),
            "out" => Some(Self::Out),
            _ => None,
        }
    }
}

impl fmt::Display for Direction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::In => write!(f, "IN"),
            Self::Out => write!(f, "OUT"),
        }
    }
}

/// A USB endpoint with bandwidth-relevant attributes.
#[derive(Debug, Clone)]
pub struct Endpoint {
    /// Endpoint address (e.g., 0x81 = IN endpoint 1, 0x02 = OUT endpoint 2).
    pub address: u8,
    /// Transfer type.
    pub transfer_type: TransferType,
    /// Direction.
    pub direction: Direction,
    /// Maximum packet size in bytes (from wMaxPacketSize).
    /// For high-speed, this includes the multiplier in bits 12:11.
    pub max_packet_size: u16,
    /// Polling interval (raw bInterval value from descriptor).
    pub b_interval: u8,
    /// Human-readable interval string from sysfs (e.g., "4ms", "125us").
    pub interval_str: String,
}

impl Endpoint {
    /// Calculate bandwidth consumption in bits per second.
    /// Only meaningful for Interrupt and Isochronous endpoints.
    pub fn bandwidth_bps(&self, device_speed: UsbSpeed) -> u64 {
        if !self.transfer_type.reserves_bandwidth() {
            return 0;
        }

        let interval_us = self.interval_us(device_speed);
        if interval_us == 0 {
            return 0;
        }

        // For high-speed, wMaxPacketSize bits 12:11 encode additional transactions
        // per microframe (0 = 1, 1 = 2, 2 = 3 transactions).
        let mult = self.multiplier();
        let packet_size = self.base_packet_size();

        // Bandwidth = (packet_size * mult * 8 bits) * (1_000_000 / interval_us)
        let bits_per_interval = packet_size as u64 * mult as u64 * 8;
        bits_per_interval * 1_000_000 / interval_us
    }

    /// Extract base packet size (bits 10:0 of wMaxPacketSize).
    fn base_packet_size(&self) -> u16 {
        self.max_packet_size & 0x07FF
    }

    /// Extract multiplier from wMaxPacketSize bits 12:11 (for high-speed).
    /// Returns 1, 2, or 3.
    fn multiplier(&self) -> u16 {
        let mult_bits = (self.max_packet_size >> 11) & 0x03;
        if mult_bits == 0 { 1 } else { mult_bits + 1 }
    }

    /// Calculate polling interval in microseconds.
    fn interval_us(&self, device_speed: UsbSpeed) -> u64 {
        match device_speed {
            UsbSpeed::Low | UsbSpeed::Full => {
                // Full/Low speed: bInterval is in milliseconds (1-255).
                // bInterval of 0 is invalid, treat as 1.
                let interval_ms = if self.b_interval == 0 {
                    1
                } else {
                    self.b_interval as u64
                };
                interval_ms * 1000
            }
            UsbSpeed::High | UsbSpeed::Super | UsbSpeed::SuperPlus | UsbSpeed::SuperPlus2 => {
                // High/Super speed: interval = 2^(bInterval-1) * 125µs.
                // bInterval range is 1-16, representing 125µs to 4096ms.
                if self.b_interval == 0 {
                    return 125; // Minimum interval
                }
                let exponent = (self.b_interval - 1).min(15) as u32;
                (1u64 << exponent) * 125
            }
        }
    }

    /// Endpoint number (address without direction bit).
    pub fn number(&self) -> u8 {
        self.address & 0x0F
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "EP{:02X} {} {} {}B @ {}",
            self.address,
            self.transfer_type,
            self.direction,
            self.base_packet_size(),
            self.interval_str
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bandwidth_calculation() {
        // Interrupt endpoint: 64 bytes, 8ms interval at full speed
        let ep = Endpoint {
            address: 0x81,
            transfer_type: TransferType::Interrupt,
            direction: Direction::In,
            max_packet_size: 64,
            b_interval: 8,
            interval_str: "8ms".to_string(),
        };

        // 64 bytes * 8 bits = 512 bits per transfer
        // 8ms interval = 125 transfers/second
        // 512 * 125 = 64000 bps = 64 Kbps
        let bw = ep.bandwidth_bps(UsbSpeed::Full);
        assert_eq!(bw, 64_000);
    }

    #[test]
    fn test_high_speed_interval() {
        let ep = Endpoint {
            address: 0x81,
            transfer_type: TransferType::Interrupt,
            direction: Direction::In,
            max_packet_size: 64,
            b_interval: 4, // 2^(4-1) * 125µs = 1000µs = 1ms
            interval_str: "1ms".to_string(),
        };

        // 64 bytes * 8 bits = 512 bits per ms = 512 Kbps
        let bw = ep.bandwidth_bps(UsbSpeed::High);
        assert_eq!(bw, 512_000);
    }

    #[test]
    fn test_bulk_no_bandwidth() {
        let ep = Endpoint {
            address: 0x02,
            transfer_type: TransferType::Bulk,
            direction: Direction::Out,
            max_packet_size: 512,
            b_interval: 0,
            interval_str: "0ms".to_string(),
        };

        assert_eq!(ep.bandwidth_bps(UsbSpeed::High), 0);
    }
}
