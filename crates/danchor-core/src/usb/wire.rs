use std::io::{Read, Write};

use plist::{Dictionary, Value};

use super::error::UsbError;

const HEADER_LEN: usize = 16;
/// usbmuxd protocol version 1: messages are framed with this header and
/// carry an XML plist payload (as opposed to version 0's raw binary
/// messages, which nothing modern speaks anymore).
const PROTOCOL_VERSION: u32 = 1;
/// The only message type used at protocol version 1 - the *actual* message
/// kind lives in the plist payload's `MessageType` key instead.
const MESSAGE_PLIST: u32 = 8;

struct UsbmuxdHeader {
    length: u32,
    version: u32,
    message: u32,
    tag: u32,
}

impl UsbmuxdHeader {
    fn encode(&self) -> [u8; HEADER_LEN] {
        let mut buf = [0u8; HEADER_LEN];
        buf[0..4].copy_from_slice(&self.length.to_le_bytes());
        buf[4..8].copy_from_slice(&self.version.to_le_bytes());
        buf[8..12].copy_from_slice(&self.message.to_le_bytes());
        buf[12..16].copy_from_slice(&self.tag.to_le_bytes());
        buf
    }

    fn decode(buf: &[u8; HEADER_LEN]) -> Self {
        Self {
            length: u32::from_le_bytes(buf[0..4].try_into().unwrap()),
            version: u32::from_le_bytes(buf[4..8].try_into().unwrap()),
            message: u32::from_le_bytes(buf[8..12].try_into().unwrap()),
            tag: u32::from_le_bytes(buf[12..16].try_into().unwrap()),
        }
    }
}

/// Writes one framed plist message. Generic over `Write` so this can be
/// exercised in tests against an in-memory buffer, not just a real socket.
pub fn write_message<W: Write>(
    writer: &mut W,
    tag: u32,
    dict: &Dictionary,
) -> Result<(), UsbError> {
    let mut payload = Vec::new();
    Value::Dictionary(dict.clone()).to_writer_xml(&mut payload)?;

    let header = UsbmuxdHeader {
        length: (HEADER_LEN + payload.len()) as u32,
        version: PROTOCOL_VERSION,
        message: MESSAGE_PLIST,
        tag,
    };

    writer.write_all(&header.encode())?;
    writer.write_all(&payload)?;
    Ok(())
}

/// Reads one framed plist message, returning its tag and decoded
/// dictionary. Generic over `Read` for the same reason as `write_message`.
pub fn read_message<R: Read>(reader: &mut R) -> Result<(u32, Dictionary), UsbError> {
    let mut header_buf = [0u8; HEADER_LEN];
    reader.read_exact(&mut header_buf)?;
    let header = UsbmuxdHeader::decode(&header_buf);

    let payload_len = (header.length as usize)
        .checked_sub(HEADER_LEN)
        .ok_or(UsbError::MissingField("header.length"))?;
    let mut payload = vec![0u8; payload_len];
    reader.read_exact(&mut payload)?;

    let value = Value::from_reader_xml(&payload[..])?;
    let dict = value
        .into_dictionary()
        .ok_or(UsbError::MissingField("root dictionary"))?;
    Ok((header.tag, dict))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn round_trips_a_message() {
        let mut dict = Dictionary::new();
        dict.insert(
            "MessageType".to_string(),
            Value::String("Result".to_string()),
        );
        dict.insert("Number".to_string(), Value::Integer(0.into()));

        let mut buf = Vec::new();
        write_message(&mut buf, 42, &dict).unwrap();

        let (tag, decoded) = read_message(&mut Cursor::new(buf)).unwrap();
        assert_eq!(tag, 42);
        assert_eq!(
            decoded.get("MessageType").unwrap().as_string(),
            Some("Result")
        );
        assert_eq!(
            decoded.get("Number").unwrap().as_unsigned_integer(),
            Some(0)
        );
    }

    #[test]
    fn rejects_truncated_stream() {
        let mut dict = Dictionary::new();
        dict.insert(
            "MessageType".to_string(),
            Value::String("Result".to_string()),
        );

        let mut buf = Vec::new();
        write_message(&mut buf, 1, &dict).unwrap();
        buf.truncate(buf.len() - 1);

        let err = read_message(&mut Cursor::new(buf)).unwrap_err();
        assert!(matches!(err, UsbError::Io(_)));
    }

    #[test]
    fn rejects_header_only_stream() {
        let err = read_message(&mut Cursor::new(vec![0u8; 4])).unwrap_err();
        assert!(matches!(err, UsbError::Io(_)));
    }
}
