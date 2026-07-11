//! UniFFI surface exposed to mobile clients.
//!
//! Deliberately minimal for now: just enough to encode a `Ping`, decode a
//! `Pong`, and compute unicast subnet-scan candidates for the first
//! connectivity milestone. Discovery is otherwise *not* exposed here -
//! Android uses its native `NsdManager` for mDNS (OS-integration territory,
//! per the same split as touch injection/video), and only the actual
//! wire-protocol bytes / portable subnet math need to come from shared
//! Rust.

use std::net::Ipv4Addr;

use danchor_core::protocol::{Packet, PacketBody};

uniffi::setup_scaffolding!();

/// The UDP port DAnchor listens on, both for the wire protocol and mDNS's
/// SRV record - shared so mobile clients don't need to hardcode it too.
#[uniffi::export]
pub fn default_port() -> u16 {
    danchor_core::transport::DEFAULT_PORT
}

/// Computes candidate host addresses to unicast-probe in `local_ip`'s
/// subnet, for use as a discovery fallback when mDNS doesn't work. Returns
/// an empty list for unparseable input, an invalid `prefix_len`, or a
/// subnet too large to scan - see `danchor_core::discovery::scan_candidates`.
#[uniffi::export]
pub fn scan_candidates(local_ip: String, prefix_len: u8) -> Vec<String> {
    let Ok(ip) = local_ip.parse::<Ipv4Addr>() else {
        return Vec::new();
    };
    danchor_core::discovery::scan_candidates(ip, prefix_len)
        .into_iter()
        .map(|ip| ip.to_string())
        .collect()
}

#[derive(uniffi::Record)]
pub struct PongReply {
    pub sequence: u32,
    pub timestamp_ms: u64,
    /// The responding desktop's stable identity - see
    /// `danchor_core::protocol::PongInfo` for why this rides along on every
    /// Ping/Pong exchange rather than only being available via mDNS TXT
    /// records (which don't always resolve, see conventions.toon).
    pub device_id: String,
    pub device_name: String,
    pub device_icon: String,
}

/// Builds the wire bytes for a `Ping` packet, ready to send over a UDP
/// socket to `transport::DEFAULT_PORT` on a discovered peer.
#[uniffi::export]
pub fn encode_ping(sequence: u32, timestamp_ms: u64) -> Vec<u8> {
    Packet {
        sequence,
        body: PacketBody::Ping(timestamp_ms),
    }
    .encode()
    .expect("a Ping packet's fixed 8-byte payload can never exceed the wire format's length limit")
}

/// Decodes a received datagram, returning `Some` only if it's a
/// well-formed `Pong` reply.
#[uniffi::export]
pub fn decode_pong(bytes: Vec<u8>) -> Option<PongReply> {
    let packet = Packet::decode(&bytes).ok()?;
    let PacketBody::Pong(info) = packet.body else {
        return None;
    };
    Some(PongReply {
        sequence: packet.sequence,
        timestamp_ms: info.timestamp_ms,
        device_id: info.device_id,
        device_name: info.device_name,
        device_icon: info.device_icon,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use danchor_core::transport::DeviceIdentity;

    fn identity() -> DeviceIdentity<'static> {
        DeviceIdentity {
            device_id: "550e8400-e29b-41d4-a716-446655440000",
            device_name: "My Desktop",
            device_icon: "desktop",
        }
    }

    #[test]
    fn ping_then_pong_round_trips() {
        let ping_bytes = encode_ping(7, 123456);

        // Simulate the desktop's real Ping->Pong responder rather than
        // hand-building a Pong, so this test also catches drift between
        // the FFI surface and `transport::handle_datagram`.
        let pong_bytes = danchor_core::transport::handle_datagram(&ping_bytes, identity()).unwrap();

        let reply = decode_pong(pong_bytes).unwrap();
        assert_eq!(reply.sequence, 7);
        assert_eq!(reply.timestamp_ms, 123456);
        assert_eq!(reply.device_id, "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(reply.device_name, "My Desktop");
        assert_eq!(reply.device_icon, "desktop");
    }

    #[test]
    fn decode_pong_rejects_non_pong_bytes() {
        assert!(decode_pong(encode_ping(1, 1)).is_none());
        assert!(decode_pong(vec![0xff, 0x00]).is_none());
    }

    #[test]
    fn scan_candidates_parses_and_excludes_self() {
        let candidates = scan_candidates("192.168.0.10".to_string(), 24);
        assert_eq!(candidates.len(), 253);
        assert!(!candidates.contains(&"192.168.0.10".to_string()));
        assert!(candidates.contains(&"192.168.0.1".to_string()));
    }

    #[test]
    fn scan_candidates_rejects_unparseable_ip() {
        assert!(scan_candidates("not-an-ip".to_string(), 24).is_empty());
    }

    #[test]
    fn default_port_matches_transport_constant() {
        assert_eq!(default_port(), danchor_core::transport::DEFAULT_PORT);
    }
}
