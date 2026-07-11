//! Local network device discovery via mDNS/DNS-SD (`_danchor._tcp.local.`).
//!
//! Unified across transports on purpose: USB carries traffic over an
//! RNDIS/NCM gadget-mode virtual network interface, so from mDNS's point of
//! view it's just another interface, and no separate USB-specific discovery
//! path is needed.

mod advertiser;
mod device_info;
mod error;
mod mdns_backend;
mod peer;
mod registry;
mod scan;

pub use advertiser::ServiceAdvertiser;
pub use device_info::{DeviceInfo, SERVICE_TYPE};
pub use error::TxtParseError;
pub use mdns_backend::{MdnsAdvertiser, MdnsBrowser};
pub use peer::{DiscoveredPeer, parse_resolved};
pub use registry::PeerRegistry;
pub use scan::{MAX_SCAN_HOSTS, scan_candidates};
