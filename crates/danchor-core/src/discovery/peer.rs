use std::collections::HashMap;
use std::net::IpAddr;

use super::device_info::DeviceInfo;
use super::error::TxtParseError;

/// A DAnchor desktop discovered on the network, fully resolved (host,
/// addresses, and a parsed `DeviceInfo`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredPeer {
    /// The mDNS instance's full name, e.g. `"My Desktop._danchor._tcp.local."`.
    /// Stable identity for a peer across updates - used as the registry key.
    pub fullname: String,
    pub host: String,
    pub port: u16,
    pub addresses: Vec<IpAddr>,
    pub device: DeviceInfo,
}

/// Builds a `DiscoveredPeer` from a resolved service's raw fields.
///
/// Takes primitives rather than an `mdns_sd::ResolvedService` directly so
/// this parsing/validation logic can be unit tested without depending on
/// that crate's types.
pub fn parse_resolved(
    fullname: &str,
    host: &str,
    port: u16,
    addresses: impl IntoIterator<Item = IpAddr>,
    txt: &HashMap<String, String>,
) -> Result<DiscoveredPeer, TxtParseError> {
    let device = DeviceInfo::from_txt_properties(txt)?;
    Ok(DiscoveredPeer {
        fullname: fullname.to_string(),
        host: host.to_string(),
        port,
        addresses: addresses.into_iter().collect(),
        device,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn txt(protocol_version: u8) -> HashMap<String, String> {
        DeviceInfo {
            protocol_version,
            device_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            device_name: "My Desktop".to_string(),
            device_icon: "desktop".to_string(),
        }
        .to_txt_properties()
    }

    #[test]
    fn parses_a_valid_peer() {
        let addr: IpAddr = "192.168.1.42".parse().unwrap();
        let peer = parse_resolved(
            "My Desktop._danchor._tcp.local.",
            "my-desktop.local.",
            9876,
            [addr],
            &txt(1),
        )
        .unwrap();

        assert_eq!(peer.fullname, "My Desktop._danchor._tcp.local.");
        assert_eq!(peer.host, "my-desktop.local.");
        assert_eq!(peer.port, 9876);
        assert_eq!(peer.addresses, vec![addr]);
        assert_eq!(peer.device.protocol_version, 1);
    }

    #[test]
    fn propagates_txt_parse_errors() {
        let err = parse_resolved(
            "My Desktop._danchor._tcp.local.",
            "my-desktop.local.",
            9876,
            [],
            &HashMap::new(),
        )
        .unwrap_err();

        assert_eq!(err, TxtParseError::MissingField("pv"));
    }
}
