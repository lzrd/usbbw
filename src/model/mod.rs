//! USB data model types.

pub mod bandwidth;
pub mod endpoint;
pub mod speed;
pub mod topology;

pub use bandwidth::{BandwidthPool, format_bps};
pub use endpoint::{Direction, Endpoint, TransferType};
pub use speed::UsbSpeed;
pub use topology::{
    ControllerId, ControllerType, DevicePath, PhysicalLocation, PortInfo, PortState, UsbBus,
    UsbController, UsbDevice, UsbTopology, format_bandwidth,
};
