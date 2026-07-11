use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;

use crate::protocol::{Packet, PacketBody};
use crate::security::{self, HandshakeSession, SecureSession};

use super::responder::{self, DeviceIdentity};
use super::socket::DatagramSocket;

/// Per-peer connection state. A peer with no entry at all is just a
/// stranger sending plain `Ping`s - handled by the existing stateless
/// `responder::handle_datagram`, unaffected by any of this.
enum PeerConnection {
    /// A Noise handshake is in progress with this peer. Boxed since
    /// `HandshakeSession` is much larger than `Established`'s fields -
    /// otherwise every `PeerConnection` (including established ones) would
    /// pay for the biggest variant's size.
    Handshaking(Box<HandshakeSession>),
    /// The handshake completed. `next_send_sequence` is this side's own
    /// outgoing nonce counter for `SecureSession::encrypt` - independent of
    /// whatever sequence numbers the peer sends its own messages under,
    /// since each direction has its own Noise transport key.
    Established {
        session: SecureSession,
        next_send_sequence: u32,
    },
}

/// Owns per-peer Noise handshake/session state on top of the existing
/// stateless `Ping`/`Pong` responder, so the desktop can hold an encrypted
/// channel open with each connecting tablet.
///
/// Only the PSK-authenticated pattern is wired here (`HandshakeSession`'s
/// `psk` parameter) - the unauthenticated ("visible for all") pattern isn't
/// used until a real "visible for all" toggle and approval flow exist to
/// make it meaningful, so a registry with no PSK configured simply can't
/// respond to a handshake attempt at all yet.
pub struct ConnectionRegistry<'a> {
    identity: DeviceIdentity<'a>,
    local_static_key: Vec<u8>,
    psk: Option<[u8; security::PSK_LEN]>,
    peers: HashMap<SocketAddr, PeerConnection>,
}

impl<'a> ConnectionRegistry<'a> {
    pub fn new(
        identity: DeviceIdentity<'a>,
        local_static_key: Vec<u8>,
        psk: Option<[u8; security::PSK_LEN]>,
    ) -> Self {
        Self {
            identity,
            local_static_key,
            psk,
            peers: HashMap::new(),
        }
    }

    /// Given one received datagram's bytes and the address it came from,
    /// decides what (if anything) to send back. Plain `Ping`s (and anything
    /// else this registry doesn't specifically track) fall straight through
    /// to the existing stateless `responder::handle_datagram`, so today's
    /// plaintext discovery ping/pong behavior is completely unaffected.
    pub fn handle_datagram(&mut self, buf: &[u8], sender: SocketAddr) -> Option<Vec<u8>> {
        let packet = Packet::decode(buf).ok()?;
        match packet.body {
            PacketBody::Handshake(bytes) => self.handle_handshake(sender, bytes),
            PacketBody::Encrypted(bytes) => self.handle_encrypted(sender, packet.sequence, bytes),
            _ => responder::handle_datagram(buf, self.identity),
        }
    }

    /// Services exactly one incoming datagram over a real socket - the
    /// stateful counterpart to `transport::serve_one`.
    pub fn serve_one<S: DatagramSocket>(
        &mut self,
        socket: &S,
        buf: &mut [u8],
    ) -> io::Result<SocketAddr> {
        let (len, sender) = socket.recv_from(buf)?;
        if let Some(reply) = self.handle_datagram(&buf[..len], sender) {
            socket.send_to(&reply, sender)?;
        }
        Ok(sender)
    }

    fn handle_handshake(&mut self, sender: SocketAddr, bytes: Vec<u8>) -> Option<Vec<u8>> {
        let mut handshake = match self.peers.remove(&sender) {
            Some(PeerConnection::Handshaking(handshake)) => handshake,
            // Either a brand-new peer, or one that already has an
            // established session sending a fresh Handshake (e.g. a
            // reconnect) - either way, start over.
            Some(PeerConnection::Established { .. }) | None => {
                let psk = self.psk?;
                Box::new(HandshakeSession::responder(&self.local_static_key, Some(&psk)).ok()?)
            }
        };

        handshake.read_next(&bytes).ok()?;

        if handshake.is_finished() {
            let session = handshake.into_session().ok()?;
            self.peers.insert(
                sender,
                PeerConnection::Established {
                    session,
                    next_send_sequence: 0,
                },
            );
            return None;
        }

        if !handshake.is_my_turn() {
            self.peers
                .insert(sender, PeerConnection::Handshaking(handshake));
            return None;
        }

        let reply = handshake.write_next().ok()?;
        self.peers
            .insert(sender, PeerConnection::Handshaking(handshake));
        Packet {
            sequence: 0,
            body: PacketBody::Handshake(reply),
        }
        .encode()
        .ok()
    }

    /// Encrypts `body` under `addr`'s established secure session and wraps
    /// it in an outer `Packet` ready to send, using (and advancing) that
    /// peer's own outgoing sequence counter. `None` if there's no
    /// established session for `addr` yet. This is the desktop's
    /// *proactive* send path (e.g. pushing captured video frames) as
    /// opposed to `handle_datagram`'s reactive request/reply path - both
    /// share the same per-peer `next_send_sequence` counter so a video
    /// stream and, say, an encrypted Ping reply never reuse a nonce.
    pub fn send_to_established(&mut self, addr: &SocketAddr, body: PacketBody) -> Option<Vec<u8>> {
        let PeerConnection::Established {
            session,
            next_send_sequence,
        } = self.peers.get_mut(addr)?
        else {
            return None;
        };

        let plaintext = Packet { sequence: 0, body }.encode().ok()?;

        let send_sequence = *next_send_sequence;
        *next_send_sequence += 1;
        let ciphertext = session.encrypt(send_sequence, &plaintext).ok()?;

        Packet {
            sequence: send_sequence,
            body: PacketBody::Encrypted(ciphertext),
        }
        .encode()
        .ok()
    }

    /// A short hex fingerprint of `addr`'s established secure-session peer
    /// key, if any - purely for human-readable logging (e.g. confirming a
    /// handshake completed for a given peer without printing the full key).
    pub fn established_peer_fingerprint(&self, addr: &SocketAddr) -> Option<String> {
        let PeerConnection::Established { session, .. } = self.peers.get(addr)? else {
            return None;
        };
        let key = session.remote_static_key()?;
        Some(key.iter().take(4).map(|b| format!("{b:02x}")).collect())
    }

    fn handle_encrypted(
        &mut self,
        sender: SocketAddr,
        sequence: u32,
        bytes: Vec<u8>,
    ) -> Option<Vec<u8>> {
        let PeerConnection::Established {
            session,
            next_send_sequence,
        } = self.peers.get_mut(&sender)?
        else {
            return None;
        };

        let inner_bytes = session.decrypt(sequence, &bytes).ok()?;
        let reply_bytes = responder::handle_datagram(&inner_bytes, self.identity)?;

        let send_sequence = *next_send_sequence;
        *next_send_sequence += 1;
        let ciphertext = session.encrypt(send_sequence, &reply_bytes).ok()?;

        Packet {
            sequence: send_sequence,
            body: PacketBody::Encrypted(ciphertext),
        }
        .encode()
        .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::PongInfo;
    use crate::security::generate_static_keypair;

    fn identity() -> DeviceIdentity<'static> {
        DeviceIdentity {
            device_id: "550e8400-e29b-41d4-a716-446655440000",
            device_name: "My Desktop",
            device_icon: "desktop",
        }
    }

    fn addr() -> SocketAddr {
        "192.168.1.50:9999".parse().unwrap()
    }

    /// Drives a full 3-message `Noise_XXpsk0` handshake between a
    /// manually-created client-side `HandshakeSession` (standing in for the
    /// tablet) and `registry` (acting as the desktop), exchanging exactly
    /// the bytes a real socket would carry. Returns the client's completed
    /// `SecureSession`, or `None` if the registry rejected/ignored message 1.
    fn client_secure_session(
        registry: &mut ConnectionRegistry,
        sender: SocketAddr,
        client_static_key: &[u8],
        client_psk: Option<&[u8; security::PSK_LEN]>,
    ) -> Option<SecureSession> {
        let mut client = HandshakeSession::initiator(client_static_key, client_psk).unwrap();

        let msg1 = client.write_next().unwrap();
        let packet1 = Packet {
            sequence: 0,
            body: PacketBody::Handshake(msg1),
        }
        .encode()
        .unwrap();
        let reply1 = registry.handle_datagram(&packet1, sender)?;
        let PacketBody::Handshake(msg2) = Packet::decode(&reply1).unwrap().body else {
            panic!("expected a handshake reply");
        };
        client.read_next(&msg2).unwrap();

        // Message 3 is XX's last message - the registry has nothing left
        // to say afterward.
        let msg3 = client.write_next().unwrap();
        let packet3 = Packet {
            sequence: 0,
            body: PacketBody::Handshake(msg3),
        }
        .encode()
        .unwrap();
        assert!(registry.handle_datagram(&packet3, sender).is_none());

        assert!(client.is_finished());
        Some(client.into_session().unwrap())
    }

    #[test]
    fn established_session_round_trips_an_encrypted_ping() {
        let psk = [7u8; security::PSK_LEN];
        let desktop_keys = generate_static_keypair().unwrap();
        let client_keys = generate_static_keypair().unwrap();

        let mut registry = ConnectionRegistry::new(identity(), desktop_keys.private, Some(psk));
        let client_session =
            client_secure_session(&mut registry, addr(), &client_keys.private, Some(&psk))
                .expect("handshake should succeed with a matching PSK");

        let ping = Packet {
            sequence: 1,
            body: PacketBody::Ping(123456),
        }
        .encode()
        .unwrap();
        let ciphertext = client_session.encrypt(0, &ping).unwrap();
        let outer = Packet {
            sequence: 0,
            body: PacketBody::Encrypted(ciphertext),
        }
        .encode()
        .unwrap();

        let reply = registry
            .handle_datagram(&outer, addr())
            .expect("an established session should answer an encrypted Ping");
        let PacketBody::Encrypted(reply_ciphertext) = Packet::decode(&reply).unwrap().body else {
            panic!("expected an encrypted reply");
        };
        let reply_plaintext = client_session.decrypt(0, &reply_ciphertext).unwrap();
        let inner = Packet::decode(&reply_plaintext).unwrap();
        assert_eq!(
            inner.body,
            PacketBody::Pong(PongInfo {
                timestamp_ms: 123456,
                device_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
                device_name: "My Desktop".to_string(),
                device_icon: "desktop".to_string(),
            })
        );
    }

    #[test]
    fn send_to_established_pushes_an_arbitrary_packet_to_the_peer() {
        let psk = [7u8; security::PSK_LEN];
        let desktop_keys = generate_static_keypair().unwrap();
        let client_keys = generate_static_keypair().unwrap();

        let mut registry = ConnectionRegistry::new(identity(), desktop_keys.private, Some(psk));
        let client_session =
            client_secure_session(&mut registry, addr(), &client_keys.private, Some(&psk))
                .expect("handshake should succeed with a matching PSK");

        let sent = registry
            .send_to_established(&addr(), PacketBody::Ping(999))
            .expect("an established peer should accept a proactive send");
        let outer = Packet::decode(&sent).unwrap();
        let PacketBody::Encrypted(ciphertext) = outer.body else {
            panic!("expected an encrypted packet");
        };
        let plaintext = client_session.decrypt(outer.sequence, &ciphertext).unwrap();
        assert_eq!(
            Packet::decode(&plaintext).unwrap().body,
            PacketBody::Ping(999)
        );

        // A second call advances the sequence rather than reusing it.
        let sent2 = registry
            .send_to_established(&addr(), PacketBody::Ping(1000))
            .unwrap();
        let outer2 = Packet::decode(&sent2).unwrap();
        assert_ne!(outer.sequence, outer2.sequence);
    }

    #[test]
    fn send_to_established_returns_none_for_an_unknown_peer() {
        let desktop_keys = generate_static_keypair().unwrap();
        let mut registry = ConnectionRegistry::new(identity(), desktop_keys.private, None);
        assert!(
            registry
                .send_to_established(&addr(), PacketBody::Ping(1))
                .is_none()
        );
    }

    #[test]
    fn mismatched_psk_handshake_is_rejected() {
        let desktop_keys = generate_static_keypair().unwrap();
        let client_keys = generate_static_keypair().unwrap();
        let mut registry = ConnectionRegistry::new(
            identity(),
            desktop_keys.private,
            Some([1u8; security::PSK_LEN]),
        );

        let mut client =
            HandshakeSession::initiator(&client_keys.private, Some(&[2u8; security::PSK_LEN]))
                .unwrap();
        let msg1 = client.write_next().unwrap();
        let packet1 = Packet {
            sequence: 0,
            body: PacketBody::Handshake(msg1),
        }
        .encode()
        .unwrap();

        assert!(registry.handle_datagram(&packet1, addr()).is_none());
    }

    #[test]
    fn handshake_with_no_configured_psk_is_ignored() {
        let desktop_keys = generate_static_keypair().unwrap();
        let client_keys = generate_static_keypair().unwrap();
        let mut registry = ConnectionRegistry::new(identity(), desktop_keys.private, None);

        let mut client =
            HandshakeSession::initiator(&client_keys.private, Some(&[9u8; security::PSK_LEN]))
                .unwrap();
        let msg1 = client.write_next().unwrap();
        let packet1 = Packet {
            sequence: 0,
            body: PacketBody::Handshake(msg1),
        }
        .encode()
        .unwrap();

        assert!(registry.handle_datagram(&packet1, addr()).is_none());
    }

    #[test]
    fn plain_ping_still_answered_directly() {
        let desktop_keys = generate_static_keypair().unwrap();
        let mut registry = ConnectionRegistry::new(identity(), desktop_keys.private, None);

        let ping = Packet {
            sequence: 5,
            body: PacketBody::Ping(42),
        }
        .encode()
        .unwrap();
        let reply = registry.handle_datagram(&ping, addr()).unwrap();
        let reply = Packet::decode(&reply).unwrap();
        assert_eq!(
            reply.body,
            PacketBody::Pong(PongInfo {
                timestamp_ms: 42,
                device_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
                device_name: "My Desktop".to_string(),
                device_icon: "desktop".to_string(),
            })
        );
    }
}
