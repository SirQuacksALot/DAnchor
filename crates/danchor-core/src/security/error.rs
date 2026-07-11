use std::fmt;

/// Errors from the Noise handshake/session layer. Wraps `snow::Error`
/// rather than re-exporting it directly, so callers outside this module
/// don't need a direct dependency on `snow`'s error type.
#[derive(Debug)]
pub enum SecurityError {
    /// The underlying Noise operation failed - a malformed message, a
    /// failed decryption/authentication check (including a mismatched
    /// PSK), or similar. `snow` doesn't implement `PartialEq` on its own
    /// error type, so this is deliberately opaque rather than matched on.
    Noise(snow::Error),
    /// `write_next`/`read_next` called when it isn't this side's turn.
    NotMyTurn,
    /// `into_session` called before the handshake finished.
    HandshakeNotFinished,
}

impl fmt::Display for SecurityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Noise(err) => write!(f, "noise protocol error: {err}"),
            Self::NotMyTurn => write!(f, "handshake message requested/processed out of turn"),
            Self::HandshakeNotFinished => write!(f, "handshake has not finished yet"),
        }
    }
}

impl std::error::Error for SecurityError {}

impl From<snow::Error> for SecurityError {
    fn from(err: snow::Error) -> Self {
        Self::Noise(err)
    }
}
