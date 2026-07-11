//! UDP transport: actually sends/receives `protocol::Packet` bytes over the
//! network, on top of a `DatagramSocket` (a thin trait mirroring
//! `std::net::UdpSocket`'s own API as the one real I/O boundary).

mod responder;
mod server;
mod socket;

pub use responder::{DeviceIdentity, handle_datagram};
pub use server::serve_one;
pub use socket::DatagramSocket;

/// Default UDP port DAnchor listens on; also what's advertised as the mDNS
/// SRV record port.
pub const DEFAULT_PORT: u16 = 47420;
