use crate::protocol::{Packet, PacketBody, PongInfo};

/// The identity a `Ping` responder embeds in its `Pong` reply - borrowed
/// strings so callers (the desktop's serve loop) don't need to
/// allocate/clone on every single incoming datagram.
#[derive(Debug, Clone, Copy)]
pub struct DeviceIdentity<'a> {
    pub device_id: &'a str,
    pub device_name: &'a str,
    pub device_icon: &'a str,
}

/// Given one received datagram's bytes, decides what (if anything) to send
/// back. Pure function - no I/O - so it's fully unit testable.
///
/// For now this only answers `Ping` with `Pong`; other packet types
/// (video/touch) aren't wired onto this path yet and are ignored rather
/// than erroring, since unexpected/malformed input arriving on a UDP socket
/// is routine and shouldn't be treated as fatal.
pub fn handle_datagram(buf: &[u8], identity: DeviceIdentity) -> Option<Vec<u8>> {
    let packet = Packet::decode(buf).ok()?;

    let PacketBody::Ping(timestamp_ms) = packet.body else {
        return None;
    };

    let pong = Packet {
        sequence: packet.sequence,
        body: PacketBody::Pong(PongInfo {
            timestamp_ms,
            device_id: identity.device_id.to_string(),
            device_name: identity.device_name.to_string(),
            device_icon: identity.device_icon.to_string(),
        }),
    };
    pong.encode().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::TouchEvent;
    use crate::protocol::TouchPhase;

    fn identity() -> DeviceIdentity<'static> {
        DeviceIdentity {
            device_id: "550e8400-e29b-41d4-a716-446655440000",
            device_name: "My Desktop",
            device_icon: "desktop",
        }
    }

    #[test]
    fn answers_ping_with_pong() {
        let ping = Packet {
            sequence: 5,
            body: PacketBody::Ping(123456),
        };
        let reply_bytes = handle_datagram(&ping.encode().unwrap(), identity()).unwrap();

        let reply = Packet::decode(&reply_bytes).unwrap();
        assert_eq!(reply.sequence, 5);
        assert_eq!(
            reply.body,
            PacketBody::Pong(PongInfo {
                timestamp_ms: 123456,
                device_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
                device_name: "My Desktop".to_string(),
                device_icon: "desktop".to_string(),
            })
        );
    }

    #[test]
    fn ignores_non_ping_packets() {
        let touch = Packet {
            sequence: 1,
            body: PacketBody::Touch(TouchEvent {
                touch_id: 0,
                phase: TouchPhase::Down,
                x: 0,
                y: 0,
                pressure: 0,
                timestamp_ms: 0,
            }),
        };
        assert!(handle_datagram(&touch.encode().unwrap(), identity()).is_none());

        let pong = Packet {
            sequence: 1,
            body: PacketBody::Pong(PongInfo {
                timestamp_ms: 1,
                device_id: String::new(),
                device_name: String::new(),
                device_icon: String::new(),
            }),
        };
        assert!(handle_datagram(&pong.encode().unwrap(), identity()).is_none());
    }

    #[test]
    fn ignores_garbage_bytes_without_panicking() {
        assert!(handle_datagram(&[0xff, 0x00, 0x01], identity()).is_none());
        assert!(handle_datagram(&[], identity()).is_none());
    }
}
