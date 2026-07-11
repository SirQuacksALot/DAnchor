use std::io::{Read, Write};

use plist::Dictionary;

use super::device::{DeviceRecord, parse_attached, parse_detached, parse_device_list};
use super::error::UsbError;
use super::message::{
    ConnectResult, connect_request, list_devices_request, listen_request, message_type,
    parse_result,
};
use super::wire::{read_message, write_message};

#[cfg(unix)]
pub const USBMUXD_SOCKET_PATH: &str = "/var/run/usbmuxd";

/// Opens a connection to the real usbmuxd daemon. Not unit tested - it
/// needs a real socket and a running daemon, neither available in CI. Every
/// function below it (`list_devices`, `connect_to_device`,
/// `DeviceListener`) is generic over `Read + Write` and fully testable
/// against an in-memory stream instead.
#[cfg(unix)]
pub fn connect_daemon() -> std::io::Result<std::os::unix::net::UnixStream> {
    std::os::unix::net::UnixStream::connect(USBMUXD_SOCKET_PATH)
}

fn expect_success(dict: &Dictionary) -> Result<(), UsbError> {
    match parse_result(dict)? {
        ConnectResult::Success => Ok(()),
        ConnectResult::Failed(code) => Err(UsbError::ConnectFailed(code)),
    }
}

/// Requests the list of currently attached devices over a fresh usbmuxd
/// connection.
pub fn list_devices<S: Read + Write>(stream: &mut S) -> Result<Vec<DeviceRecord>, UsbError> {
    write_message(stream, 1, &list_devices_request())?;
    let (_tag, response) = read_message(stream)?;
    parse_device_list(&response)
}

/// Requests a byte-stream tunnel to `port` on `device_id`. On success,
/// `stream` is handed back ready for direct, unframed I/O: usbmuxd stops
/// framing at that point, and everything read/written afterward is the raw
/// tunneled connection to that port on the device.
pub fn connect_to_device<S: Read + Write>(
    mut stream: S,
    device_id: u32,
    port: u16,
) -> Result<S, UsbError> {
    write_message(&mut stream, 1, &connect_request(device_id, port))?;
    let (_tag, response) = read_message(&mut stream)?;
    expect_success(&response)?;
    Ok(stream)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceEvent {
    Attached(DeviceRecord),
    Detached(u32),
}

/// A subscription to device attach/detach notifications. Once started, the
/// underlying connection is dedicated to notifications and can't be reused
/// for other requests - that's how usbmuxd's `Listen` command works.
pub struct DeviceListener<S> {
    stream: S,
}

impl<S: Read + Write> DeviceListener<S> {
    pub fn start(mut stream: S) -> Result<Self, UsbError> {
        write_message(&mut stream, 1, &listen_request())?;
        let (_tag, response) = read_message(&mut stream)?;
        expect_success(&response)?;
        Ok(Self { stream })
    }

    /// Blocks until the next attach/detach notification arrives.
    pub fn next_event(&mut self) -> Result<DeviceEvent, UsbError> {
        let (_tag, dict) = read_message(&mut self.stream)?;
        match message_type(&dict)? {
            "Attached" => Ok(DeviceEvent::Attached(parse_attached(&dict)?)),
            "Detached" => Ok(DeviceEvent::Detached(parse_detached(&dict)?)),
            other => Err(UsbError::UnexpectedMessageType(other.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plist::Value;
    use std::io::Cursor;

    /// An in-memory duplex stream: reads come from a preloaded buffer of
    /// "what the fake daemon sent", writes are captured so tests can assert
    /// on exactly what request bytes we produced.
    #[derive(Debug, Default)]
    struct MockStream {
        incoming: Cursor<Vec<u8>>,
        pub outgoing: Vec<u8>,
    }

    impl MockStream {
        fn with_incoming(bytes: Vec<u8>) -> Self {
            Self {
                incoming: Cursor::new(bytes),
                outgoing: Vec::new(),
            }
        }
    }

    impl Read for MockStream {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.incoming.read(buf)
        }
    }

    impl Write for MockStream {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.outgoing.extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn encode(tag: u32, dict: &Dictionary) -> Vec<u8> {
        let mut buf = Vec::new();
        write_message(&mut buf, tag, dict).unwrap();
        buf
    }

    fn result_message(number: u64) -> Dictionary {
        let mut dict = Dictionary::new();
        dict.insert(
            "MessageType".to_string(),
            Value::String("Result".to_string()),
        );
        dict.insert("Number".to_string(), Value::Integer(number.into()));
        dict
    }

    #[test]
    fn list_devices_sends_the_right_request_and_parses_the_response() {
        let mut response = Dictionary::new();
        response.insert("DeviceList".to_string(), Value::Array(vec![]));
        let mut stream = MockStream::with_incoming(encode(1, &response));

        let devices = list_devices(&mut stream).unwrap();
        assert!(devices.is_empty());

        let (_tag, sent) = read_message(&mut Cursor::new(stream.outgoing)).unwrap();
        assert_eq!(message_type(&sent).unwrap(), "ListDevices");
    }

    #[test]
    fn connect_to_device_succeeds_and_sends_the_swapped_port() {
        let stream = MockStream::with_incoming(encode(1, &result_message(0)));

        let mut tunneled = connect_to_device(stream, 7, 8000).unwrap();

        let (_tag, sent) =
            read_message(&mut Cursor::new(std::mem::take(&mut tunneled.outgoing))).unwrap();
        assert_eq!(message_type(&sent).unwrap(), "Connect");
        assert_eq!(sent.get("DeviceID").unwrap().as_unsigned_integer(), Some(7));
        assert_eq!(
            sent.get("PortNumber").unwrap().as_unsigned_integer(),
            Some(8000u16.swap_bytes() as u64)
        );
    }

    #[test]
    fn connect_to_device_surfaces_a_refusal() {
        let stream = MockStream::with_incoming(encode(1, &result_message(3)));

        let err = connect_to_device(stream, 7, 8000).unwrap_err();
        assert!(matches!(err, UsbError::ConnectFailed(3)));
    }

    #[test]
    fn listener_reports_attach_and_detach_events_in_order() {
        let mut attached = Dictionary::new();
        attached.insert(
            "MessageType".to_string(),
            Value::String("Attached".to_string()),
        );
        attached.insert("DeviceID".to_string(), Value::Integer(4.into()));
        let mut props = Dictionary::new();
        props.insert("SerialNumber".to_string(), Value::String("XYZ".to_string()));
        props.insert(
            "ConnectionType".to_string(),
            Value::String("USB".to_string()),
        );
        attached.insert("Properties".to_string(), Value::Dictionary(props));

        let mut detached = Dictionary::new();
        detached.insert(
            "MessageType".to_string(),
            Value::String("Detached".to_string()),
        );
        detached.insert("DeviceID".to_string(), Value::Integer(4.into()));

        let mut incoming = encode(1, &result_message(0));
        incoming.extend(encode(2, &attached));
        incoming.extend(encode(3, &detached));

        let stream = MockStream::with_incoming(incoming);
        let mut listener = DeviceListener::start(stream).unwrap();

        match listener.next_event().unwrap() {
            DeviceEvent::Attached(device) => {
                assert_eq!(device.device_id, 4);
                assert_eq!(device.serial_number, "XYZ");
            }
            other => panic!("expected Attached, got {other:?}"),
        }
        assert_eq!(listener.next_event().unwrap(), DeviceEvent::Detached(4));
    }
}
