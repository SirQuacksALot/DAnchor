use plist::{Dictionary, Value};

use super::error::UsbError;

const CLIENT_VERSION_STRING: &str = "danchor-0.1";
const PROG_NAME: &str = "danchor";

pub(crate) fn get_string<'a>(
    dict: &'a Dictionary,
    field: &'static str,
) -> Result<&'a str, UsbError> {
    dict.get(field)
        .and_then(Value::as_string)
        .ok_or(UsbError::MissingField(field))
}

pub(crate) fn get_u32(dict: &Dictionary, field: &'static str) -> Result<u32, UsbError> {
    dict.get(field)
        .and_then(Value::as_unsigned_integer)
        .map(|v| v as u32)
        .ok_or(UsbError::MissingField(field))
}

pub(crate) fn message_type(dict: &Dictionary) -> Result<&str, UsbError> {
    get_string(dict, "MessageType")
}

fn base_request(message_type: &str) -> Dictionary {
    let mut dict = Dictionary::new();
    dict.insert(
        "MessageType".to_string(),
        Value::String(message_type.to_string()),
    );
    dict.insert(
        "ClientVersionString".to_string(),
        Value::String(CLIENT_VERSION_STRING.to_string()),
    );
    dict.insert("ProgName".to_string(), Value::String(PROG_NAME.to_string()));
    dict
}

pub fn list_devices_request() -> Dictionary {
    base_request("ListDevices")
}

/// Subscribes the connection to attach/detach notifications. Per usbmuxd's
/// protocol, a connection that sends `Listen` is dedicated to notifications
/// from then on and can't be reused for other requests.
pub fn listen_request() -> Dictionary {
    base_request("Listen")
}

/// Requests a byte-stream tunnel to `port` on `device_id`.
pub fn connect_request(device_id: u32, port: u16) -> Dictionary {
    let mut dict = base_request("Connect");
    dict.insert("DeviceID".to_string(), Value::Integer(device_id.into()));
    // usbmuxd expects the port pre-swapped to network byte order (like a C
    // client would get for free from `htons()`), so on our little-endian
    // wire it comes out byte-reversed from the plain port number.
    dict.insert(
        "PortNumber".to_string(),
        Value::Integer(port.swap_bytes().into()),
    );
    dict
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectResult {
    Success,
    /// usbmuxd's own header only documents a handful of these codes (e.g.
    /// 2 = BadDevice, 3 = ConnectionRefused); passed through as-is.
    Failed(u64),
}

pub fn parse_result(dict: &Dictionary) -> Result<ConnectResult, UsbError> {
    let ty = message_type(dict)?;
    if ty != "Result" {
        return Err(UsbError::UnexpectedMessageType(ty.to_string()));
    }

    let number = dict
        .get("Number")
        .and_then(Value::as_unsigned_integer)
        .ok_or(UsbError::MissingField("Number"))?;

    Ok(if number == 0 {
        ConnectResult::Success
    } else {
        ConnectResult::Failed(number)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_request_byte_swaps_the_port() {
        let dict = connect_request(7, 8000);
        assert_eq!(dict.get("DeviceID").unwrap().as_unsigned_integer(), Some(7));
        // 8000 = 0x1F40; byte-swapped as a u16 that's 0x401F = 16415.
        assert_eq!(
            dict.get("PortNumber").unwrap().as_unsigned_integer(),
            Some(16415)
        );
    }

    #[test]
    fn list_devices_request_has_expected_shape() {
        let dict = list_devices_request();
        assert_eq!(message_type(&dict).unwrap(), "ListDevices");
        assert_eq!(get_string(&dict, "ProgName").unwrap(), PROG_NAME);
    }

    #[test]
    fn parse_result_reports_success() {
        let mut dict = Dictionary::new();
        dict.insert(
            "MessageType".to_string(),
            Value::String("Result".to_string()),
        );
        dict.insert("Number".to_string(), Value::Integer(0.into()));

        assert_eq!(parse_result(&dict).unwrap(), ConnectResult::Success);
    }

    #[test]
    fn parse_result_reports_failure_code() {
        let mut dict = Dictionary::new();
        dict.insert(
            "MessageType".to_string(),
            Value::String("Result".to_string()),
        );
        dict.insert("Number".to_string(), Value::Integer(3.into()));

        assert_eq!(parse_result(&dict).unwrap(), ConnectResult::Failed(3));
    }

    #[test]
    fn parse_result_rejects_wrong_message_type() {
        let mut dict = Dictionary::new();
        dict.insert(
            "MessageType".to_string(),
            Value::String("Attached".to_string()),
        );

        let err = parse_result(&dict).unwrap_err();
        assert!(matches!(err, UsbError::UnexpectedMessageType(ty) if ty == "Attached"));
    }
}
