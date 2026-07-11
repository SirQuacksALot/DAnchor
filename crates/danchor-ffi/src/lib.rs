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
use std::sync::{Arc, Mutex};

use danchor_core::protocol::{Packet, PacketBody};
use danchor_core::security::{self, HandshakeSession, SecureSession};

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

#[derive(uniffi::Record)]
pub struct NoiseKeypair {
    pub private: Vec<u8>,
    pub public: Vec<u8>,
}

/// Generates a fresh Noise static keypair for a `NoiseHandshake` initiator.
/// Android doesn't persist this today - there's no long-term "device Noise
/// identity" concept yet, so a new one is generated per connection attempt
/// (see `danchor_core::security::generate_static_keypair`).
#[uniffi::export]
pub fn generate_noise_keypair() -> NoiseKeypair {
    let keypair = security::generate_static_keypair()
        .expect("generating a fresh Curve25519 keypair should never fail");
    NoiseKeypair {
        private: keypair.private,
        public: keypair.public,
    }
}

/// Builds the wire bytes for one message of a Noise handshake in progress -
/// see `NoiseHandshake` for the state machine that produces these.
#[uniffi::export]
pub fn encode_handshake(sequence: u32, message: Vec<u8>) -> Vec<u8> {
    Packet {
        sequence,
        body: PacketBody::Handshake(message),
    }
    .encode()
    .expect("a handshake message never exceeds the wire format's length limit")
}

/// Decodes a received datagram, returning the inner handshake message bytes
/// only if it's a well-formed `Handshake` packet.
#[uniffi::export]
pub fn decode_handshake(bytes: Vec<u8>) -> Option<Vec<u8>> {
    let packet = Packet::decode(&bytes).ok()?;
    let PacketBody::Handshake(message) = packet.body else {
        return None;
    };
    Some(message)
}

#[derive(uniffi::Record)]
pub struct EncryptedPacket {
    pub sequence: u32,
    pub ciphertext: Vec<u8>,
}

/// Builds the wire bytes for one message encrypted under an established
/// `SecureChannel` - `sequence` doubles as the Noise nonce (see
/// `danchor_core::security::SecureSession::encrypt`).
#[uniffi::export]
pub fn encode_encrypted(sequence: u32, ciphertext: Vec<u8>) -> Vec<u8> {
    Packet {
        sequence,
        body: PacketBody::Encrypted(ciphertext),
    }
    .encode()
    .expect("an encrypted payload never exceeds the wire format's length limit")
}

/// Decodes a received datagram, returning the sequence (Noise nonce) and
/// ciphertext only if it's a well-formed `Encrypted` packet.
#[uniffi::export]
pub fn decode_encrypted(bytes: Vec<u8>) -> Option<EncryptedPacket> {
    let packet = Packet::decode(&bytes).ok()?;
    let PacketBody::Encrypted(ciphertext) = packet.body else {
        return None;
    };
    Some(EncryptedPacket {
        sequence: packet.sequence,
        ciphertext,
    })
}

/// A Noise handshake in progress, exposed to Kotlin as a stateful object
/// (an `Arc`-backed handle, per UniFFI's object model) wrapping
/// `danchor_core::security::HandshakeSession`. Every fallible operation
/// collapses to `None`/`false` rather than throwing across the FFI
/// boundary, since a mismatched PSK or malformed message is routine on an
/// untrusted network, not exceptional - matching this crate's existing
/// `decode_pong`-style error handling.
#[derive(uniffi::Object)]
pub struct NoiseHandshake {
    // `Mutex<Option<..>>` so `into_session` can `.take()` the state out from
    // behind the shared `&self` every UniFFI object method receives.
    state: Mutex<Option<HandshakeSession>>,
}

#[uniffi::export]
impl NoiseHandshake {
    /// Starts a handshake as the initiator - the tablet's role in DAnchor's
    /// model (the desktop only ever responds, see
    /// `danchor_core::transport::ConnectionRegistry`). `psk` is the
    /// household trust secret for the authenticated fast path; passing
    /// `None` builds an unauthenticated attempt that no desktop currently
    /// accepts (see `ConnectionRegistry`'s doc comment on why that pattern
    /// isn't wired in yet).
    #[uniffi::constructor]
    pub fn initiator(local_private_key: Vec<u8>, psk: Option<Vec<u8>>) -> Arc<Self> {
        let psk_array: Option<[u8; security::PSK_LEN]> = psk.and_then(|p| p.try_into().ok());
        let session = HandshakeSession::initiator(&local_private_key, psk_array.as_ref())
            .expect("initiator() requires a valid static key and, if present, a 32-byte psk");
        Arc::new(Self {
            state: Mutex::new(Some(session)),
        })
    }

    pub fn is_finished(&self) -> bool {
        self.state
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| s.is_finished())
            .unwrap_or(false)
    }

    pub fn is_my_turn(&self) -> bool {
        self.state
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| s.is_my_turn())
            .unwrap_or(false)
    }

    /// Produces this side's next handshake message, or `None` if it isn't
    /// this side's turn or the handshake was already consumed.
    pub fn write_next(&self) -> Option<Vec<u8>> {
        let mut guard = self.state.lock().unwrap();
        guard.as_mut()?.write_next().ok()
    }

    /// Processes the peer's handshake message, returning whether it was
    /// accepted - a mismatched PSK or malformed message returns `false`
    /// rather than throwing.
    pub fn read_next(&self, message: Vec<u8>) -> bool {
        let mut guard = self.state.lock().unwrap();
        guard
            .as_mut()
            .map(|s| s.read_next(&message).is_ok())
            .unwrap_or(false)
    }

    /// Finishes the handshake into a `SecureChannel`, or `None` if it
    /// hasn't finished yet or was already consumed by a prior call.
    pub fn into_session(&self) -> Option<Arc<SecureChannel>> {
        let mut guard = self.state.lock().unwrap();
        let session = guard.take()?;
        if !session.is_finished() {
            *guard = Some(session);
            return None;
        }
        session
            .into_session()
            .ok()
            .map(|session| Arc::new(SecureChannel { inner: session }))
    }
}

/// An established secure channel, exposed to Kotlin as a stateful object
/// wrapping `danchor_core::security::SecureSession`.
#[derive(uniffi::Object)]
pub struct SecureChannel {
    inner: SecureSession,
}

#[uniffi::export]
impl SecureChannel {
    /// Encrypts `plaintext` under `sequence` as the Noise nonce. Callers
    /// MUST use each `sequence` at most once per direction (see
    /// `danchor_core::security::SecureSession::encrypt`).
    pub fn encrypt(&self, sequence: u32, plaintext: Vec<u8>) -> Option<Vec<u8>> {
        self.inner.encrypt(sequence, &plaintext).ok()
    }

    /// Decrypts `ciphertext` that was encrypted under `sequence`.
    pub fn decrypt(&self, sequence: u32, ciphertext: Vec<u8>) -> Option<Vec<u8>> {
        self.inner.decrypt(sequence, &ciphertext).ok()
    }
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

    fn addr() -> std::net::SocketAddr {
        "192.168.1.50:9999".parse().unwrap()
    }

    // Drives a full handshake+encrypted-Ping round trip using ONLY the FFI
    // surface on the client side (NoiseHandshake/SecureChannel plus the
    // encode/decode helpers) against a real `ConnectionRegistry` acting as
    // the desktop - catches drift between the FFI wrappers and the
    // underlying `danchor_core` types they wrap, the same way
    // `ping_then_pong_round_trips` does for plain Ping/Pong.
    #[test]
    fn ffi_handshake_round_trips_an_encrypted_ping() {
        let desktop_keys = security::generate_static_keypair().unwrap();
        let client_keys = generate_noise_keypair();
        let psk = vec![9u8; security::PSK_LEN];

        let mut registry = danchor_core::transport::ConnectionRegistry::new(
            identity(),
            desktop_keys.private,
            Some(psk.clone().try_into().unwrap()),
        );

        let client = NoiseHandshake::initiator(client_keys.private, Some(psk));

        let msg1 = client
            .write_next()
            .expect("initiator writes message 1 first");
        let reply1 = registry
            .handle_datagram(&encode_handshake(0, msg1), addr())
            .expect("registry should reply to message 1");
        let msg2 = decode_handshake(reply1).expect("reply should be a handshake message");
        assert!(client.read_next(msg2));

        let msg3 = client
            .write_next()
            .expect("initiator writes message 3 last");
        assert!(
            registry
                .handle_datagram(&encode_handshake(0, msg3), addr())
                .is_none(),
            "message 3 is XX's last message - no reply expected"
        );

        assert!(client.is_finished());
        let channel = client.into_session().expect("handshake should be finished");

        let ping = encode_ping(1, 555);
        let ciphertext = channel.encrypt(0, ping).unwrap();
        let reply = registry
            .handle_datagram(&encode_encrypted(0, ciphertext), addr())
            .expect("an established session should answer an encrypted Ping");

        let encrypted_reply = decode_encrypted(reply).expect("reply should be an encrypted packet");
        let plaintext = channel
            .decrypt(encrypted_reply.sequence, encrypted_reply.ciphertext)
            .unwrap();
        let pong = decode_pong(plaintext).expect("decrypted payload should be a Pong");
        assert_eq!(pong.timestamp_ms, 555);
        assert_eq!(pong.device_name, "My Desktop");
    }

    #[test]
    fn noise_handshake_rejects_mismatched_psk() {
        let client_keys = generate_noise_keypair();
        let client =
            NoiseHandshake::initiator(client_keys.private, Some(vec![1u8; security::PSK_LEN]));

        let desktop_keys = security::generate_static_keypair().unwrap();
        let mut registry = danchor_core::transport::ConnectionRegistry::new(
            identity(),
            desktop_keys.private,
            Some([2u8; security::PSK_LEN]),
        );

        let msg1 = client.write_next().unwrap();
        assert!(
            registry
                .handle_datagram(&encode_handshake(0, msg1), addr())
                .is_none()
        );
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
