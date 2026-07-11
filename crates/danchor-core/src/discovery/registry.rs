use std::collections::HashMap;

use super::peer::DiscoveredPeer;

/// Tracks the set of currently-known DAnchor peers on the network, built up
/// from a stream of found/removed events (e.g. from an mDNS browser).
///
/// This is the hardware-independent half of browsing: it has no idea where
/// events come from, so it's fully unit testable.
#[derive(Debug, Default)]
pub struct PeerRegistry {
    peers: HashMap<String, DiscoveredPeer>,
}

impl PeerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a newly resolved (or re-resolved) peer, keyed by its
    /// `fullname`. Returns `true` if this peer wasn't already known.
    pub fn found(&mut self, peer: DiscoveredPeer) -> bool {
        self.peers.insert(peer.fullname.clone(), peer).is_none()
    }

    /// Forgets a peer by its `fullname`. Returns `true` if it was known.
    pub fn removed(&mut self, fullname: &str) -> bool {
        self.peers.remove(fullname).is_some()
    }

    pub fn peers(&self) -> impl Iterator<Item = &DiscoveredPeer> {
        self.peers.values()
    }

    pub fn get(&self, fullname: &str) -> Option<&DiscoveredPeer> {
        self.peers.get(fullname)
    }

    pub fn len(&self) -> usize {
        self.peers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.peers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::device_info::DeviceInfo;

    fn peer(fullname: &str, protocol_version: u8) -> DiscoveredPeer {
        DiscoveredPeer {
            fullname: fullname.to_string(),
            host: "host.local.".to_string(),
            port: 1234,
            addresses: vec![],
            device: DeviceInfo {
                protocol_version,
                device_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
                device_name: "My Desktop".to_string(),
                device_icon: "desktop".to_string(),
            },
        }
    }

    #[test]
    fn found_adds_a_new_peer() {
        let mut registry = PeerRegistry::new();
        assert!(registry.found(peer("a", 1)));
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.get("a").unwrap().device.protocol_version, 1);
    }

    #[test]
    fn found_again_updates_rather_than_duplicates() {
        let mut registry = PeerRegistry::new();
        registry.found(peer("a", 1));
        let was_new = registry.found(peer("a", 2));

        assert!(!was_new);
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.get("a").unwrap().device.protocol_version, 2);
    }

    #[test]
    fn removed_forgets_a_known_peer() {
        let mut registry = PeerRegistry::new();
        registry.found(peer("a", 1));

        assert!(registry.removed("a"));
        assert!(registry.is_empty());
    }

    #[test]
    fn removed_unknown_peer_is_a_no_op() {
        let mut registry = PeerRegistry::new();
        assert!(!registry.removed("nonexistent"));
        assert!(registry.is_empty());
    }

    #[test]
    fn tracks_multiple_peers_independently() {
        let mut registry = PeerRegistry::new();
        registry.found(peer("a", 1));
        registry.found(peer("b", 2));
        registry.removed("a");

        let remaining: Vec<_> = registry.peers().map(|p| p.fullname.as_str()).collect();
        assert_eq!(remaining, vec!["b"]);
    }
}
