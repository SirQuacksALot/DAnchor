//! Noise-protocol-based secure channel, via the `snow` crate rather than a
//! hand-rolled scheme - this is exactly the "mutual auth + encrypted
//! channel over an untrusted transport" problem Noise is designed for, and
//! `snow` is a mature, widely-used pure-Rust implementation.
//!
//! Deliberately pure/no-I/O, same split as every other external-boundary
//! module in this crate (`usb`, `discovery`'s mdns backend): this module
//! only drives the handshake and encrypts/decrypts bytes. Persisting a
//! device's static keypair, actually sending handshake messages over a
//! socket, and deciding which pattern to use for a given peer are all the
//! caller's job (`danchor-desktop`/the transport layer), kept out of here
//! so the crypto logic itself stays fully unit-testable.

mod error;

pub use error::SecurityError;

use snow::{Builder, HandshakeState, StatelessTransportState};

/// Noise pattern for a session authenticated by a pre-shared secret - the
/// "basic trust" fast path when a client already holds this desktop's
/// trust secret. `psk0` placement mixes the secret in before the very
/// first message, so a wrong secret is rejected immediately rather than
/// partway through the handshake.
const PATTERN_PSK: &str = "Noise_XXpsk0_25519_ChaChaPoly_BLAKE2s";

/// Noise pattern for a session with no pre-shared secret - used only when
/// a desktop has opted into "visible for all". Completes a secure,
/// mutually-authenticated-by-key (not by secret) channel first, so the
/// subsequent identity exchange and approval prompt happen over an already
/// encrypted link rather than in the clear.
const PATTERN_UNAUTHENTICATED: &str = "Noise_XX_25519_ChaChaPoly_BLAKE2s";

/// Byte length of a pre-shared key, fixed by the Noise spec.
pub const PSK_LEN: usize = 32;

/// Handshake messages for these patterns are small (a 25519 key plus, at
/// most, one encrypted 25519 key) - 1024 bytes is generous headroom.
const MAX_HANDSHAKE_MESSAGE_LEN: usize = 1024;

/// A freshly generated Curve25519 keypair for use as a device's stable
/// Noise identity. Generating one is pure/deterministic-free (uses the
/// resolver's CSPRNG); persisting it is the caller's responsibility.
pub struct StaticKeypair {
    pub private: Vec<u8>,
    pub public: Vec<u8>,
}

/// Generates a new static keypair for `HandshakeSession::initiator`/
/// `responder`. Call once per device and persist the result - a device's
/// Noise identity should be stable across restarts, not regenerated every
/// launch.
pub fn generate_static_keypair() -> Result<StaticKeypair, SecurityError> {
    let keypair = Builder::new(PATTERN_UNAUTHENTICATED.parse()?).generate_keypair()?;
    Ok(StaticKeypair {
        private: keypair.private,
        public: keypair.public,
    })
}

/// A Noise handshake in progress. Feed the peer's messages in via
/// `read_next`, pull this side's own messages out via `write_next`, and
/// once `is_finished()`, call `into_session` to get a `SecureSession`.
pub struct HandshakeSession {
    state: HandshakeState,
}

impl HandshakeSession {
    /// Starts a handshake as the connection initiator (always the tablet in
    /// DAnchor's model - the desktop only ever responds). `psk` selects the
    /// pattern: `Some` for the pre-shared-secret fast path, `None` for the
    /// unauthenticated ("visible for all") path.
    pub fn initiator(
        local_private_key: &[u8],
        psk: Option<&[u8; PSK_LEN]>,
    ) -> Result<Self, SecurityError> {
        Self::build(local_private_key, psk, true)
    }

    /// Starts a handshake as the connection responder (always the desktop).
    pub fn responder(
        local_private_key: &[u8],
        psk: Option<&[u8; PSK_LEN]>,
    ) -> Result<Self, SecurityError> {
        Self::build(local_private_key, psk, false)
    }

    fn build(
        local_private_key: &[u8],
        psk: Option<&[u8; PSK_LEN]>,
        initiator: bool,
    ) -> Result<Self, SecurityError> {
        let pattern = if psk.is_some() {
            PATTERN_PSK
        } else {
            PATTERN_UNAUTHENTICATED
        };
        let mut builder = Builder::new(pattern.parse()?).local_private_key(local_private_key)?;
        if let Some(psk) = psk {
            builder = builder.psk(0, psk)?;
        }
        let state = if initiator {
            builder.build_initiator()?
        } else {
            builder.build_responder()?
        };
        Ok(Self { state })
    }

    /// Whether it's this side's turn to send the next handshake message.
    pub fn is_my_turn(&self) -> bool {
        self.state.is_my_turn()
    }

    /// Whether the handshake has completed (both sides can now derive
    /// transport keys via `into_session`).
    pub fn is_finished(&self) -> bool {
        self.state.is_handshake_finished()
    }

    /// The peer's static public key, once the message carrying it has been
    /// processed (for the XX-family patterns used here: after message 2 on
    /// the initiator's side, after message 3 on the responder's). `None`
    /// before then.
    pub fn remote_static_key(&self) -> Option<&[u8]> {
        self.state.get_remote_static()
    }

    /// Produces this side's next handshake message. Only valid when
    /// `is_my_turn()`.
    pub fn write_next(&mut self) -> Result<Vec<u8>, SecurityError> {
        if !self.is_my_turn() {
            return Err(SecurityError::NotMyTurn);
        }
        let mut buf = [0u8; MAX_HANDSHAKE_MESSAGE_LEN];
        let len = self.state.write_message(&[], &mut buf)?;
        Ok(buf[..len].to_vec())
    }

    /// Processes the peer's handshake message. Only valid when it's *not*
    /// this side's turn (i.e. a message is expected from the peer).
    pub fn read_next(&mut self, message: &[u8]) -> Result<(), SecurityError> {
        if self.is_my_turn() {
            return Err(SecurityError::NotMyTurn);
        }
        let mut discard = [0u8; MAX_HANDSHAKE_MESSAGE_LEN];
        self.state.read_message(message, &mut discard)?;
        Ok(())
    }

    /// Finishes the handshake into a `SecureSession`. Errors if the
    /// handshake hasn't completed yet.
    pub fn into_session(self) -> Result<SecureSession, SecurityError> {
        if !self.is_finished() {
            return Err(SecurityError::HandshakeNotFinished);
        }
        Ok(SecureSession {
            state: self.state.into_stateless_transport_mode()?,
        })
    }
}

/// An established secure channel. Stateless-transport mode (not the
/// counter-tracking `TransportState`) since DAnchor runs over UDP, an
/// unreliable transport where the caller - not `snow` - needs to control
/// the per-message nonce, per Noise's own guidance for this exact case.
pub struct SecureSession {
    state: StatelessTransportState,
}

impl SecureSession {
    /// Encrypts `plaintext` under `sequence` as the Noise nonce, reusing
    /// the wire protocol's own per-sender `Packet.sequence` rather than
    /// maintaining a second counter. Callers MUST use each `sequence`
    /// value at most once per direction - Noise's replay/reuse protection
    /// depends on the nonce never repeating.
    pub fn encrypt(&self, sequence: u32, plaintext: &[u8]) -> Result<Vec<u8>, SecurityError> {
        let mut buf = vec![0u8; plaintext.len() + 16]; // +16 for the AEAD tag
        let len = self
            .state
            .write_message(sequence as u64, plaintext, &mut buf)?;
        buf.truncate(len);
        Ok(buf)
    }

    /// Decrypts `ciphertext` that was encrypted under `sequence`.
    pub fn decrypt(&self, sequence: u32, ciphertext: &[u8]) -> Result<Vec<u8>, SecurityError> {
        let mut buf = vec![0u8; ciphertext.len()];
        let len = self
            .state
            .read_message(sequence as u64, ciphertext, &mut buf)?;
        buf.truncate(len);
        Ok(buf)
    }

    /// The peer's static public key, learned during the handshake that
    /// produced this session.
    pub fn remote_static_key(&self) -> Option<&[u8]> {
        self.state.get_remote_static()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drive_to_completion(
        mut a: HandshakeSession,
        mut b: HandshakeSession,
    ) -> Result<(SecureSession, SecureSession), SecurityError> {
        // XX is 3 messages: initiator -> responder -> initiator. Whichever
        // side's is_my_turn() is true acts; this loop doesn't assume which
        // one starts, so it works for both initiator-first call order and
        // (if ever needed) a responder that's handed a pre-sent message.
        while !a.is_finished() || !b.is_finished() {
            if a.is_my_turn() {
                let msg = a.write_next()?;
                b.read_next(&msg)?;
            } else {
                let msg = b.write_next()?;
                a.read_next(&msg)?;
            }
        }
        Ok((a.into_session()?, b.into_session()?))
    }

    #[test]
    fn unauthenticated_handshake_establishes_a_working_session() {
        let initiator_keys = generate_static_keypair().unwrap();
        let responder_keys = generate_static_keypair().unwrap();

        let initiator = HandshakeSession::initiator(&initiator_keys.private, None).unwrap();
        let responder = HandshakeSession::responder(&responder_keys.private, None).unwrap();

        let (client, server) = drive_to_completion(initiator, responder).unwrap();

        let ciphertext = client.encrypt(0, b"hello").unwrap();
        let plaintext = server.decrypt(0, &ciphertext).unwrap();
        assert_eq!(plaintext, b"hello");
    }

    #[test]
    fn unauthenticated_handshake_learns_the_peers_static_key() {
        let initiator_keys = generate_static_keypair().unwrap();
        let responder_keys = generate_static_keypair().unwrap();

        let initiator = HandshakeSession::initiator(&initiator_keys.private, None).unwrap();
        let responder = HandshakeSession::responder(&responder_keys.private, None).unwrap();

        let (client, server) = drive_to_completion(initiator, responder).unwrap();

        assert_eq!(
            client.remote_static_key().unwrap(),
            responder_keys.public.as_slice()
        );
        assert_eq!(
            server.remote_static_key().unwrap(),
            initiator_keys.public.as_slice()
        );
    }

    #[test]
    fn matching_psk_establishes_a_working_session() {
        let initiator_keys = generate_static_keypair().unwrap();
        let responder_keys = generate_static_keypair().unwrap();
        let psk = [42u8; PSK_LEN];

        let initiator = HandshakeSession::initiator(&initiator_keys.private, Some(&psk)).unwrap();
        let responder = HandshakeSession::responder(&responder_keys.private, Some(&psk)).unwrap();

        let (client, server) = drive_to_completion(initiator, responder).unwrap();

        let ciphertext = client.encrypt(0, b"trusted").unwrap();
        assert_eq!(server.decrypt(0, &ciphertext).unwrap(), b"trusted");
    }

    #[test]
    fn mismatched_psk_is_rejected() {
        let initiator_keys = generate_static_keypair().unwrap();
        let responder_keys = generate_static_keypair().unwrap();

        let mut initiator =
            HandshakeSession::initiator(&initiator_keys.private, Some(&[1u8; PSK_LEN])).unwrap();
        let mut responder =
            HandshakeSession::responder(&responder_keys.private, Some(&[2u8; PSK_LEN])).unwrap();

        // psk0 placement mixes the secret in before message 1, so the
        // mismatch is caught on the very first message the responder reads.
        let first_message = initiator.write_next().unwrap();
        let result = responder.read_next(&first_message);
        assert!(matches!(result, Err(SecurityError::Noise(_))));
    }

    #[test]
    fn write_next_out_of_turn_is_rejected() {
        let keys = generate_static_keypair().unwrap();
        let mut initiator = HandshakeSession::initiator(&keys.private, None).unwrap();
        initiator.write_next().unwrap(); // consumes the initiator's turn
        assert!(matches!(
            initiator.write_next(),
            Err(SecurityError::NotMyTurn)
        ));
    }

    #[test]
    fn into_session_before_finished_is_rejected() {
        let keys = generate_static_keypair().unwrap();
        let initiator = HandshakeSession::initiator(&keys.private, None).unwrap();
        assert!(matches!(
            initiator.into_session(),
            Err(SecurityError::HandshakeNotFinished)
        ));
    }

    #[test]
    fn decrypting_with_the_wrong_sequence_fails() {
        let initiator_keys = generate_static_keypair().unwrap();
        let responder_keys = generate_static_keypair().unwrap();
        let initiator = HandshakeSession::initiator(&initiator_keys.private, None).unwrap();
        let responder = HandshakeSession::responder(&responder_keys.private, None).unwrap();
        let (client, server) = drive_to_completion(initiator, responder).unwrap();

        let ciphertext = client.encrypt(5, b"hello").unwrap();
        assert!(server.decrypt(6, &ciphertext).is_err());
    }

    #[test]
    fn remote_static_key_is_none_before_it_is_learned() {
        let keys = generate_static_keypair().unwrap();
        let initiator = HandshakeSession::initiator(&keys.private, None).unwrap();
        assert!(initiator.remote_static_key().is_none());
    }
}
