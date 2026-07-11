use std::collections::HashMap;

use super::error::TxtParseError;

/// DAnchor's DNS-SD service type. Advertised identically regardless of
/// whether the underlying link is a real network or a USB/RNDIS gadget-mode
/// virtual network interface - mDNS just sees "an interface" either way.
pub const SERVICE_TYPE: &str = "_danchor._tcp.local.";

const KEY_PROTOCOL_VERSION: &str = "pv";
const KEY_DEVICE_ID: &str = "id";
const KEY_DEVICE_NAME: &str = "name";
const KEY_DEVICE_ICON: &str = "icon";

/// The metadata a desktop advertises about itself in its TXT record, beyond
/// what's already carried by mDNS itself (instance name, host, port).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    /// The wire protocol version this desktop speaks, so a tablet can
    /// filter out incompatible peers before attempting to connect.
    pub protocol_version: u8,
    /// Stable identity for this desktop, independent of its (renamable)
    /// display name or (DHCP-dependent) address - a random UUID generated
    /// once and persisted locally. Not yet used for anything (no pairing
    /// protocol exists yet, see .ai/tasks.toon); this is the identity that
    /// protocol will eventually carry.
    pub device_id: String,
    /// Human-readable display name, separate from the mDNS instance name so
    /// a future rename doesn't require re-advertising under a new service
    /// name.
    pub device_name: String,
    /// A coarse device-type hint (e.g. "desktop") a client can use to pick
    /// an icon - there's no customization of this yet, just a fixed default.
    pub device_icon: String,
}

impl DeviceInfo {
    pub fn to_txt_properties(&self) -> HashMap<String, String> {
        HashMap::from([
            (
                KEY_PROTOCOL_VERSION.to_string(),
                self.protocol_version.to_string(),
            ),
            (KEY_DEVICE_ID.to_string(), self.device_id.clone()),
            (KEY_DEVICE_NAME.to_string(), self.device_name.clone()),
            (KEY_DEVICE_ICON.to_string(), self.device_icon.clone()),
        ])
    }

    pub fn from_txt_properties(props: &HashMap<String, String>) -> Result<Self, TxtParseError> {
        let raw_version = props
            .get(KEY_PROTOCOL_VERSION)
            .ok_or(TxtParseError::MissingField(KEY_PROTOCOL_VERSION))?;

        let protocol_version =
            raw_version
                .parse::<u8>()
                .map_err(|_| TxtParseError::InvalidValue {
                    field: KEY_PROTOCOL_VERSION,
                    value: raw_version.clone(),
                })?;

        let device_id = props
            .get(KEY_DEVICE_ID)
            .ok_or(TxtParseError::MissingField(KEY_DEVICE_ID))?
            .clone();
        let device_name = props
            .get(KEY_DEVICE_NAME)
            .ok_or(TxtParseError::MissingField(KEY_DEVICE_NAME))?
            .clone();
        let device_icon = props
            .get(KEY_DEVICE_ICON)
            .ok_or(TxtParseError::MissingField(KEY_DEVICE_ICON))?
            .clone();

        Ok(Self {
            protocol_version,
            device_id,
            device_name,
            device_icon,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> DeviceInfo {
        DeviceInfo {
            protocol_version: 1,
            device_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            device_name: "My Desktop".to_string(),
            device_icon: "desktop".to_string(),
        }
    }

    #[test]
    fn round_trips() {
        let info = sample();
        let props = info.to_txt_properties();
        assert_eq!(DeviceInfo::from_txt_properties(&props).unwrap(), info);
    }

    #[test]
    fn rejects_missing_protocol_version() {
        let mut props = sample().to_txt_properties();
        props.remove(KEY_PROTOCOL_VERSION);
        let err = DeviceInfo::from_txt_properties(&props).unwrap_err();
        assert_eq!(err, TxtParseError::MissingField(KEY_PROTOCOL_VERSION));
    }

    #[test]
    fn rejects_missing_device_id() {
        let mut props = sample().to_txt_properties();
        props.remove(KEY_DEVICE_ID);
        let err = DeviceInfo::from_txt_properties(&props).unwrap_err();
        assert_eq!(err, TxtParseError::MissingField(KEY_DEVICE_ID));
    }

    #[test]
    fn rejects_missing_device_name() {
        let mut props = sample().to_txt_properties();
        props.remove(KEY_DEVICE_NAME);
        let err = DeviceInfo::from_txt_properties(&props).unwrap_err();
        assert_eq!(err, TxtParseError::MissingField(KEY_DEVICE_NAME));
    }

    #[test]
    fn rejects_missing_device_icon() {
        let mut props = sample().to_txt_properties();
        props.remove(KEY_DEVICE_ICON);
        let err = DeviceInfo::from_txt_properties(&props).unwrap_err();
        assert_eq!(err, TxtParseError::MissingField(KEY_DEVICE_ICON));
    }

    #[test]
    fn rejects_non_numeric_protocol_version() {
        let mut props = sample().to_txt_properties();
        props.insert(KEY_PROTOCOL_VERSION.to_string(), "not-a-number".to_string());
        let err = DeviceInfo::from_txt_properties(&props).unwrap_err();
        assert_eq!(
            err,
            TxtParseError::InvalidValue {
                field: KEY_PROTOCOL_VERSION,
                value: "not-a-number".to_string()
            }
        );
    }

    #[test]
    fn rejects_out_of_range_protocol_version() {
        let mut props = sample().to_txt_properties();
        props.insert(KEY_PROTOCOL_VERSION.to_string(), "256".to_string());
        let err = DeviceInfo::from_txt_properties(&props).unwrap_err();
        assert_eq!(
            err,
            TxtParseError::InvalidValue {
                field: KEY_PROTOCOL_VERSION,
                value: "256".to_string()
            }
        );
    }
}
