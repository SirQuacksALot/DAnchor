use std::fmt;

/// Errors that can occur while encoding or decoding wire packets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    BufferTooShort { expected: usize, actual: usize },
    InvalidMagic,
    UnsupportedVersion(u8),
    UnknownPacketType(u8),
    UnknownTouchPhase(u8),
    PayloadLengthMismatch { declared: usize, actual: usize },
    PayloadTooLarge { len: usize, max: usize },
    FragmentIndexOutOfRange { index: u16, count: u16 },
    TooManyFragments { count: usize, max: u16 },
    InvalidUtf8,
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BufferTooShort { expected, actual } => write!(
                f,
                "buffer too short: expected at least {expected} bytes, got {actual}"
            ),
            Self::InvalidMagic => write!(f, "invalid packet magic bytes"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported protocol version: {v}"),
            Self::UnknownPacketType(t) => write!(f, "unknown packet type: {t}"),
            Self::UnknownTouchPhase(p) => write!(f, "unknown touch phase: {p}"),
            Self::PayloadLengthMismatch { declared, actual } => write!(
                f,
                "payload length mismatch: header declared {declared} bytes, buffer had {actual}"
            ),
            Self::PayloadTooLarge { len, max } => {
                write!(f, "payload too large: {len} bytes exceeds max of {max}")
            }
            Self::FragmentIndexOutOfRange { index, count } => write!(
                f,
                "fragment index {index} out of range for fragment count {count}"
            ),
            Self::TooManyFragments { count, max } => write!(
                f,
                "frame split into {count} fragments, exceeding the max of {max}"
            ),
            Self::InvalidUtf8 => write!(f, "string field contained invalid UTF-8"),
        }
    }
}

impl std::error::Error for ProtocolError {}
