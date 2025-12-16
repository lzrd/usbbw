//! USB Bandwidth Visualization Tool
//!
//! A library and CLI tool for visualizing USB bandwidth allocation on Linux systems.

pub mod config;
pub mod model;
pub mod output;
pub mod sysfs;
pub mod ui;

pub use config::Config;
pub use model::{UsbBus, UsbDevice, UsbSpeed, UsbTopology};
pub use sysfs::SysfsParser;
