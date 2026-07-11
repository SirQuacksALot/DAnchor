//! DAnchor's wire protocol: a custom UDP framing (no WebRTC) for streaming
//! encoded video to the tablet and carrying touch input back, chosen for
//! full control over the encode/latency pipeline on a trusted LAN/USB link.

mod error;
mod header;
mod touch;
mod video;

pub use error::ProtocolError;
pub use touch::{TouchEvent, TouchPhase};
pub use video::{CompleteFrame, FrameReassembler, VideoFragment, fragment_frame};

use header::{HEADER_LEN, PacketHeader};

/// The wire protocol version this build speaks - matches what `discovery`
/// advertises in `DeviceInfo` so peers can pre-flight compatibility.
pub const VERSION: u8 = header::VERSION;

const PACKET_TYPE_VIDEO: u8 = 0;
const PACKET_TYPE_TOUCH: u8 = 1;
const PACKET_TYPE_PING: u8 = 2;
const PACKET_TYPE_PONG: u8 = 3;
const PACKET_TYPE_HANDSHAKE: u8 = 4;
const PACKET_TYPE_ENCRYPTED: u8 = 5;

const MAX_PAYLOAD_LEN: usize = u16::MAX as usize;

/// A `Pong` reply's payload: the echoed `Ping` timestamp for RTT
/// measurement, plus the responder's identity. Every discovery path this
/// app has (mDNS, unicast subnet scan, manual entry) converges on a
/// Ping/Pong round trip, so this is the one place a client is guaranteed to
/// learn who it's actually talking to - mDNS TXT records carry the same
/// info but only reach clients when mDNS itself works, which isn't
/// reliable on every network (see conventions.toon).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PongInfo {
    pub timestamp_ms: u64,
    pub device_id: String,
    pub device_name: String,
    pub device_icon: String,
}

/// The decoded contents of a packet, tagged by which kind of message it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PacketBody {
    /// One fragment of an encoded video frame (desktop -> tablet).
    Video(VideoFragment),
    /// One touch-point update (tablet -> desktop).
    Touch(TouchEvent),
    /// Liveness/RTT probe carrying the sender's timestamp in milliseconds.
    Ping(u64),
    /// Reply to a `Ping`, echoing back the timestamp it carried plus the
    /// responder's identity.
    Pong(PongInfo),
    /// One message of a Noise handshake in progress - opaque bytes handed
    /// directly to/from `snow`. Not further parsed here; see the `security`
    /// module for the handshake state machine that produces/consumes these.
    Handshake(Vec<u8>),
    /// A `Packet` (any other variant, itself `encode()`d) encrypted under an
    /// established `security::SecureSession`. The `sequence` field on the
    /// *outer* packet doubles as the Noise nonce for this ciphertext - see
    /// `security::SecureSession` for why that reuse is safe.
    Encrypted(Vec<u8>),
}

/// A single wire packet: a monotonic per-sender sequence number plus its
/// typed body. `sequence` is assigned by the sender and is independent of
/// any per-frame fragment numbering carried inside `PacketBody::Video`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    pub sequence: u32,
    pub body: PacketBody,
}

impl Packet {
    pub fn encode(&self) -> Result<Vec<u8>, ProtocolError> {
        let mut payload = Vec::new();
        let packet_type = match &self.body {
            PacketBody::Video(fragment) => {
                fragment.encode(&mut payload);
                PACKET_TYPE_VIDEO
            }
            PacketBody::Touch(event) => {
                event.encode(&mut payload);
                PACKET_TYPE_TOUCH
            }
            PacketBody::Ping(timestamp_ms) => {
                payload.extend_from_slice(&timestamp_ms.to_be_bytes());
                PACKET_TYPE_PING
            }
            PacketBody::Pong(info) => {
                payload.extend_from_slice(&info.timestamp_ms.to_be_bytes());
                encode_string(&mut payload, &info.device_id);
                encode_string(&mut payload, &info.device_name);
                encode_string(&mut payload, &info.device_icon);
                PACKET_TYPE_PONG
            }
            PacketBody::Handshake(bytes) => {
                payload.extend_from_slice(bytes);
                PACKET_TYPE_HANDSHAKE
            }
            PacketBody::Encrypted(bytes) => {
                payload.extend_from_slice(bytes);
                PACKET_TYPE_ENCRYPTED
            }
        };

        if payload.len() > MAX_PAYLOAD_LEN {
            return Err(ProtocolError::PayloadTooLarge {
                len: payload.len(),
                max: MAX_PAYLOAD_LEN,
            });
        }

        let header = PacketHeader {
            packet_type,
            sequence: self.sequence,
            payload_len: payload.len() as u16,
        };

        let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
        header.encode(&mut out);
        out.extend_from_slice(&payload);
        Ok(out)
    }

    pub fn decode(buf: &[u8]) -> Result<Self, ProtocolError> {
        let (header, rest) = PacketHeader::decode(buf)?;
        if rest.len() != header.payload_len as usize {
            return Err(ProtocolError::PayloadLengthMismatch {
                declared: header.payload_len as usize,
                actual: rest.len(),
            });
        }

        let body = match header.packet_type {
            PACKET_TYPE_VIDEO => PacketBody::Video(VideoFragment::decode(rest)?),
            PACKET_TYPE_TOUCH => PacketBody::Touch(TouchEvent::decode(rest)?),
            PACKET_TYPE_PING => PacketBody::Ping(decode_timestamp(rest)?),
            PACKET_TYPE_PONG => PacketBody::Pong(decode_pong_info(rest)?),
            PACKET_TYPE_HANDSHAKE => PacketBody::Handshake(rest.to_vec()),
            PACKET_TYPE_ENCRYPTED => PacketBody::Encrypted(rest.to_vec()),
            other => return Err(ProtocolError::UnknownPacketType(other)),
        };

        Ok(Self {
            sequence: header.sequence,
            body,
        })
    }
}

fn decode_timestamp(buf: &[u8]) -> Result<u64, ProtocolError> {
    if buf.len() != 8 {
        return Err(ProtocolError::PayloadLengthMismatch {
            declared: 8,
            actual: buf.len(),
        });
    }
    Ok(u64::from_be_bytes(buf.try_into().unwrap()))
}

/// Length-prefixed (u16 BE) UTF-8 string encoding, used for `PongInfo`'s
/// three identity fields - simple and hand-rolled like the rest of this
/// module, no serde needed for three short strings.
fn encode_string(out: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    out.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
    out.extend_from_slice(bytes);
}

fn decode_string(buf: &[u8]) -> Result<(String, &[u8]), ProtocolError> {
    if buf.len() < 2 {
        return Err(ProtocolError::BufferTooShort {
            expected: 2,
            actual: buf.len(),
        });
    }
    let len = u16::from_be_bytes([buf[0], buf[1]]) as usize;
    let rest = &buf[2..];
    if rest.len() < len {
        return Err(ProtocolError::BufferTooShort {
            expected: len,
            actual: rest.len(),
        });
    }
    let (str_bytes, remaining) = rest.split_at(len);
    let s = String::from_utf8(str_bytes.to_vec()).map_err(|_| ProtocolError::InvalidUtf8)?;
    Ok((s, remaining))
}

fn decode_pong_info(buf: &[u8]) -> Result<PongInfo, ProtocolError> {
    if buf.len() < 8 {
        return Err(ProtocolError::BufferTooShort {
            expected: 8,
            actual: buf.len(),
        });
    }
    let timestamp_ms = u64::from_be_bytes(buf[0..8].try_into().unwrap());
    let (device_id, rest) = decode_string(&buf[8..])?;
    let (device_name, rest) = decode_string(rest)?;
    let (device_icon, rest) = decode_string(rest)?;
    if !rest.is_empty() {
        return Err(ProtocolError::PayloadLengthMismatch {
            declared: buf.len() - rest.len(),
            actual: buf.len(),
        });
    }
    Ok(PongInfo {
        timestamp_ms,
        device_id,
        device_name,
        device_icon,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_pong_info() -> PongInfo {
        PongInfo {
            timestamp_ms: 123456,
            device_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            device_name: "My Desktop".to_string(),
            device_icon: "desktop".to_string(),
        }
    }

    #[test]
    fn video_packet_round_trips() {
        let packet = Packet {
            sequence: 99,
            body: PacketBody::Video(VideoFragment {
                frame_id: 5,
                fragment_index: 0,
                fragment_count: 1,
                keyframe: true,
                data: vec![1, 2, 3],
            }),
        };
        let encoded = packet.encode().unwrap();
        assert_eq!(Packet::decode(&encoded).unwrap(), packet);
    }

    #[test]
    fn touch_packet_round_trips() {
        let packet = Packet {
            sequence: 1,
            body: PacketBody::Touch(TouchEvent {
                touch_id: 0,
                phase: TouchPhase::Down,
                x: 100,
                y: 200,
                pressure: 128,
                timestamp_ms: 42,
            }),
        };
        let encoded = packet.encode().unwrap();
        assert_eq!(Packet::decode(&encoded).unwrap(), packet);
    }

    #[test]
    fn ping_pong_packets_round_trip() {
        let ping = Packet {
            sequence: 3,
            body: PacketBody::Ping(123456),
        };
        let pong = Packet {
            sequence: 4,
            body: PacketBody::Pong(sample_pong_info()),
        };
        assert_eq!(Packet::decode(&ping.encode().unwrap()).unwrap(), ping);
        assert_eq!(Packet::decode(&pong.encode().unwrap()).unwrap(), pong);
    }

    #[test]
    fn pong_info_round_trips_with_empty_strings() {
        let pong = Packet {
            sequence: 1,
            body: PacketBody::Pong(PongInfo {
                timestamp_ms: 0,
                device_id: String::new(),
                device_name: String::new(),
                device_icon: String::new(),
            }),
        };
        assert_eq!(Packet::decode(&pong.encode().unwrap()).unwrap(), pong);
    }

    #[test]
    fn pong_info_round_trips_with_unicode() {
        let pong = Packet {
            sequence: 1,
            body: PacketBody::Pong(PongInfo {
                timestamp_ms: 1,
                device_id: "id".to_string(),
                device_name: "Büro-Rechner 🖥️".to_string(),
                device_icon: "desktop".to_string(),
            }),
        };
        assert_eq!(Packet::decode(&pong.encode().unwrap()).unwrap(), pong);
    }

    #[test]
    fn handshake_packet_round_trips() {
        let packet = Packet {
            sequence: 0,
            body: PacketBody::Handshake(vec![1, 2, 3, 4, 5]),
        };
        let encoded = packet.encode().unwrap();
        assert_eq!(Packet::decode(&encoded).unwrap(), packet);
    }

    #[test]
    fn handshake_packet_round_trips_with_empty_bytes() {
        let packet = Packet {
            sequence: 0,
            body: PacketBody::Handshake(Vec::new()),
        };
        let encoded = packet.encode().unwrap();
        assert_eq!(Packet::decode(&encoded).unwrap(), packet);
    }

    #[test]
    fn encrypted_packet_round_trips() {
        let packet = Packet {
            sequence: 7,
            body: PacketBody::Encrypted(vec![0xde, 0xad, 0xbe, 0xef]),
        };
        let encoded = packet.encode().unwrap();
        assert_eq!(Packet::decode(&encoded).unwrap(), packet);
    }

    #[test]
    fn decode_rejects_unknown_packet_type() {
        let packet = Packet {
            sequence: 1,
            body: PacketBody::Ping(0),
        };
        let mut encoded = packet.encode().unwrap();
        encoded[3] = 0xaa; // corrupt the packet-type byte
        let err = Packet::decode(&encoded).unwrap_err();
        assert_eq!(err, ProtocolError::UnknownPacketType(0xaa));
    }

    #[test]
    fn decode_rejects_truncated_payload() {
        let packet = Packet {
            sequence: 1,
            body: PacketBody::Ping(0),
        };
        let mut encoded = packet.encode().unwrap();
        encoded.pop(); // drop the last payload byte without fixing the header
        let err = Packet::decode(&encoded).unwrap_err();
        assert_eq!(
            err,
            ProtocolError::PayloadLengthMismatch {
                declared: 8,
                actual: 7
            }
        );
    }

    #[test]
    fn decode_rejects_short_buffer() {
        let err = Packet::decode(&[1, 2, 3]).unwrap_err();
        assert_eq!(
            err,
            ProtocolError::BufferTooShort {
                expected: HEADER_LEN,
                actual: 3
            }
        );
    }

    #[test]
    fn decode_pong_rejects_truncated_string_length() {
        // A payload that's internally consistent with the header's declared
        // length (so the outer length check passes) but stops right after
        // the timestamp, before any of the three length-prefixed strings -
        // exercises decode_string's own BufferTooShort path specifically.
        let mut buf = Vec::new();
        PacketHeader {
            packet_type: PACKET_TYPE_PONG,
            sequence: 1,
            payload_len: 8,
        }
        .encode(&mut buf);
        buf.extend_from_slice(&0u64.to_be_bytes());

        let err = Packet::decode(&buf).unwrap_err();
        assert_eq!(
            err,
            ProtocolError::BufferTooShort {
                expected: 2,
                actual: 0
            }
        );
    }
}
