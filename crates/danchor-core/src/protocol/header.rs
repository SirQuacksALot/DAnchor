use super::error::ProtocolError;

/// "DA" - identifies a DAnchor wire packet so stray UDP traffic on the same
/// port is rejected instead of misparsed.
pub(crate) const MAGIC: [u8; 2] = [0x44, 0x41];
pub(crate) const VERSION: u8 = 1;
pub(crate) const HEADER_LEN: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PacketHeader {
    pub packet_type: u8,
    pub sequence: u32,
    pub payload_len: u16,
}

impl PacketHeader {
    pub(crate) fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&MAGIC);
        out.push(VERSION);
        out.push(self.packet_type);
        out.extend_from_slice(&self.sequence.to_be_bytes());
        out.extend_from_slice(&self.payload_len.to_be_bytes());
    }

    /// Decodes the fixed-size header from the front of `buf` and returns it
    /// along with the remaining, still-undecoded slice.
    pub(crate) fn decode(buf: &[u8]) -> Result<(Self, &[u8]), ProtocolError> {
        if buf.len() < HEADER_LEN {
            return Err(ProtocolError::BufferTooShort {
                expected: HEADER_LEN,
                actual: buf.len(),
            });
        }

        if buf[0..2] != MAGIC {
            return Err(ProtocolError::InvalidMagic);
        }

        let version = buf[2];
        if version != VERSION {
            return Err(ProtocolError::UnsupportedVersion(version));
        }

        let packet_type = buf[3];
        let sequence = u32::from_be_bytes(buf[4..8].try_into().unwrap());
        let payload_len = u16::from_be_bytes(buf[8..10].try_into().unwrap());

        Ok((
            Self {
                packet_type,
                sequence,
                payload_len,
            },
            &buf[HEADER_LEN..],
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let header = PacketHeader {
            packet_type: 7,
            sequence: 0xdead_beef,
            payload_len: 1234,
        };
        let mut buf = Vec::new();
        header.encode(&mut buf);
        assert_eq!(buf.len(), HEADER_LEN);

        let (decoded, rest) = PacketHeader::decode(&buf).unwrap();
        assert_eq!(decoded, header);
        assert!(rest.is_empty());
    }

    #[test]
    fn decode_leaves_trailing_bytes_untouched() {
        let header = PacketHeader {
            packet_type: 1,
            sequence: 1,
            payload_len: 3,
        };
        let mut buf = Vec::new();
        header.encode(&mut buf);
        buf.extend_from_slice(&[9, 9, 9]);

        let (_, rest) = PacketHeader::decode(&buf).unwrap();
        assert_eq!(rest, &[9, 9, 9]);
    }

    #[test]
    fn rejects_short_buffer() {
        let err = PacketHeader::decode(&[0x44, 0x41, 1]).unwrap_err();
        assert_eq!(
            err,
            ProtocolError::BufferTooShort {
                expected: HEADER_LEN,
                actual: 3
            }
        );
    }

    #[test]
    fn rejects_bad_magic() {
        let mut buf = vec![0xff, 0xff, VERSION, 0];
        buf.extend_from_slice(&[0; 6]);
        let err = PacketHeader::decode(&buf).unwrap_err();
        assert_eq!(err, ProtocolError::InvalidMagic);
    }

    #[test]
    fn rejects_unsupported_version() {
        let mut buf = vec![MAGIC[0], MAGIC[1], VERSION + 1, 0];
        buf.extend_from_slice(&[0; 6]);
        let err = PacketHeader::decode(&buf).unwrap_err();
        assert_eq!(err, ProtocolError::UnsupportedVersion(VERSION + 1));
    }
}
