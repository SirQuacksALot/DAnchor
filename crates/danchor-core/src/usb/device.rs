use plist::{Dictionary, Value};

use super::error::UsbError;
use super::message::{get_string, get_u32, message_type};

/// An iOS device currently attached over USB, as reported by usbmuxd.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceRecord {
    pub device_id: u32,
    pub serial_number: String,
    pub connection_type: String,
}

fn parse_record(dict: &Dictionary) -> Result<DeviceRecord, UsbError> {
    let device_id = get_u32(dict, "DeviceID")?;
    let properties = dict
        .get("Properties")
        .and_then(Value::as_dictionary)
        .ok_or(UsbError::MissingField("Properties"))?;

    Ok(DeviceRecord {
        device_id,
        serial_number: get_string(properties, "SerialNumber")?.to_string(),
        connection_type: get_string(properties, "ConnectionType")?.to_string(),
    })
}

pub fn parse_device_list(dict: &Dictionary) -> Result<Vec<DeviceRecord>, UsbError> {
    let list = dict
        .get("DeviceList")
        .and_then(Value::as_array)
        .ok_or(UsbError::MissingField("DeviceList"))?;

    list.iter()
        .map(|entry| {
            entry
                .as_dictionary()
                .ok_or(UsbError::MissingField("DeviceList[]"))
                .and_then(parse_record)
        })
        .collect()
}

pub fn parse_attached(dict: &Dictionary) -> Result<DeviceRecord, UsbError> {
    let ty = message_type(dict)?;
    if ty != "Attached" {
        return Err(UsbError::UnexpectedMessageType(ty.to_string()));
    }
    parse_record(dict)
}

pub fn parse_detached(dict: &Dictionary) -> Result<u32, UsbError> {
    let ty = message_type(dict)?;
    if ty != "Detached" {
        return Err(UsbError::UnexpectedMessageType(ty.to_string()));
    }
    get_u32(dict, "DeviceID")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device_dict(id: u32, serial: &str) -> Dictionary {
        let mut props = Dictionary::new();
        props.insert(
            "SerialNumber".to_string(),
            Value::String(serial.to_string()),
        );
        props.insert(
            "ConnectionType".to_string(),
            Value::String("USB".to_string()),
        );

        let mut dict = Dictionary::new();
        dict.insert("DeviceID".to_string(), Value::Integer(id.into()));
        dict.insert("Properties".to_string(), Value::Dictionary(props));
        dict
    }

    #[test]
    fn parses_a_device_list() {
        let mut root = Dictionary::new();
        root.insert(
            "DeviceList".to_string(),
            Value::Array(vec![
                Value::Dictionary(device_dict(1, "AAA")),
                Value::Dictionary(device_dict(2, "BBB")),
            ]),
        );

        let devices = parse_device_list(&root).unwrap();
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].device_id, 1);
        assert_eq!(devices[0].serial_number, "AAA");
        assert_eq!(devices[1].device_id, 2);
    }

    #[test]
    fn parses_attached_event() {
        let mut dict = device_dict(5, "CCC");
        dict.insert(
            "MessageType".to_string(),
            Value::String("Attached".to_string()),
        );

        let device = parse_attached(&dict).unwrap();
        assert_eq!(device.device_id, 5);
        assert_eq!(device.serial_number, "CCC");
    }

    #[test]
    fn parses_detached_event() {
        let mut dict = Dictionary::new();
        dict.insert(
            "MessageType".to_string(),
            Value::String("Detached".to_string()),
        );
        dict.insert("DeviceID".to_string(), Value::Integer(9.into()));

        assert_eq!(parse_detached(&dict).unwrap(), 9);
    }

    #[test]
    fn rejects_mismatched_event_type() {
        let mut dict = Dictionary::new();
        dict.insert(
            "MessageType".to_string(),
            Value::String("Detached".to_string()),
        );
        dict.insert("DeviceID".to_string(), Value::Integer(9.into()));

        let err = parse_attached(&dict).unwrap_err();
        assert!(matches!(err, UsbError::UnexpectedMessageType(ty) if ty == "Detached"));
    }
}
