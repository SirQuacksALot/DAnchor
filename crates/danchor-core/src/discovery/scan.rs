use std::net::Ipv4Addr;

/// Max number of hosts a subnet scan will enumerate, so a misconfigured or
/// unusually large subnet (e.g. a corporate /20) can't turn into thousands
/// of probes.
pub const MAX_SCAN_HOSTS: usize = 1024;

/// Computes every candidate host address to unicast-probe in the subnet
/// described by `local_ip`/`prefix_len`, excluding the network address,
/// broadcast address, and `local_ip` itself.
///
/// This exists as a fallback for when mDNS discovery doesn't work: some
/// WiFi routers don't forward multicast between wireless clients even
/// though plain unicast UDP works fine between them, so probing every host
/// in the local subnet directly sidesteps the problem entirely.
///
/// Returns an empty list for invalid input (`prefix_len` out of `1..=32`)
/// or a subnet larger than `MAX_SCAN_HOSTS` usable hosts, rather than
/// erroring - a fallback that occasionally finds nothing is fine; one that
/// panics or blocks on an enormous scan is not.
pub fn scan_candidates(local_ip: Ipv4Addr, prefix_len: u8) -> Vec<Ipv4Addr> {
    if !(1..=32).contains(&prefix_len) {
        return Vec::new();
    }

    let host_bits = 32 - u32::from(prefix_len);
    let host_count: u64 = 1u64 << host_bits;

    // Leave headroom so subnets right at the cap (e.g. exactly /22) still
    // fit after the network/broadcast addresses are excluded below.
    if host_count > MAX_SCAN_HOSTS as u64 + 2 {
        return Vec::new();
    }

    let mask = !0u32 << host_bits;
    let network = u32::from(local_ip) & mask;
    let broadcast = u64::from(network) + host_count - 1;

    // /31 and /32 have no distinct network/broadcast address (RFC 3021) -
    // every address in range is a usable host.
    let (first, last) = if prefix_len >= 31 {
        (u64::from(network), broadcast)
    } else {
        (u64::from(network) + 1, broadcast - 1)
    };

    if last < first {
        return Vec::new();
    }

    (first..=last)
        .map(|addr| Ipv4Addr::from(addr as u32))
        .filter(|&ip| ip != local_ip)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typical_home_slash_24_excludes_network_broadcast_and_self() {
        let candidates = scan_candidates(Ipv4Addr::new(192, 168, 0, 10), 24);
        assert_eq!(candidates.len(), 253); // 254 usable hosts minus self

        assert!(!candidates.contains(&Ipv4Addr::new(192, 168, 0, 0))); // network
        assert!(!candidates.contains(&Ipv4Addr::new(192, 168, 0, 255))); // broadcast
        assert!(!candidates.contains(&Ipv4Addr::new(192, 168, 0, 10))); // self
        assert!(candidates.contains(&Ipv4Addr::new(192, 168, 0, 1)));
        assert!(candidates.contains(&Ipv4Addr::new(192, 168, 0, 254)));
    }

    #[test]
    fn slash_32_has_no_other_hosts() {
        assert!(scan_candidates(Ipv4Addr::new(10, 0, 0, 5), 32).is_empty());
    }

    #[test]
    fn slash_31_point_to_point_returns_the_other_address() {
        let candidates = scan_candidates(Ipv4Addr::new(10, 0, 0, 4), 31);
        assert_eq!(candidates, vec![Ipv4Addr::new(10, 0, 0, 5)]);
    }

    #[test]
    fn refuses_to_scan_an_oversized_subnet() {
        assert!(scan_candidates(Ipv4Addr::new(10, 0, 0, 1), 16).is_empty());
    }

    #[test]
    fn rejects_invalid_prefix_lengths() {
        assert!(scan_candidates(Ipv4Addr::new(10, 0, 0, 1), 0).is_empty());
        assert!(scan_candidates(Ipv4Addr::new(10, 0, 0, 1), 33).is_empty());
    }

    #[test]
    fn slash_22_fits_right_under_the_cap() {
        // 1024 total addresses, 1022 usable hosts - should just barely scan.
        let candidates = scan_candidates(Ipv4Addr::new(172, 16, 0, 1), 22);
        assert_eq!(candidates.len(), 1021); // 1022 usable minus self
    }
}
