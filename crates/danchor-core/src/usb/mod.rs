//! Client for `usbmuxd`, the daemon that multiplexes TCP-style byte-stream
//! connections to iOS devices over USB.
//!
//! Unlike Android - which just presents a normal RNDIS/NCM network
//! interface that the existing `discovery`/`protocol` modules already work
//! over unmodified - iOS devices don't support USB gadget networking at
//! all. usbmuxd's `Connect` request is the only way to get a byte-stream
//! tunnel to a port on an attached iPad over USB, which is why this is a
//! separate module rather than something layered onto `discovery`.
//!
//! Note this only yields a TCP-like byte stream, not a datagram channel -
//! `protocol`'s custom UDP framing can't run directly over it as-is.
//!
//! IMPORTANT: the wire format here (header layout, message field names, the
//! `PortNumber` byte-swap) is modeled from public usbmuxd protocol
//! documentation, not verified against a live daemon/device in this
//! environment (no iPad or running usbmuxd available here). Validate
//! against the real thing before relying on it.

mod client;
mod device;
mod error;
mod message;
mod wire;

pub use client::{DeviceEvent, DeviceListener, connect_to_device, list_devices};
#[cfg(unix)]
pub use client::{USBMUXD_SOCKET_PATH, connect_daemon};
pub use device::DeviceRecord;
pub use error::UsbError;
pub use message::ConnectResult;
