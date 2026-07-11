//! The real discovery backend, using the `mdns-sd` crate.
//!
//! Untested by design: `ServiceDaemon` opens real multicast sockets and
//! runs a background thread, neither of which is available/deterministic
//! in a unit test. Everything with actual logic - TXT encode/decode,
//! peer parsing, peer bookkeeping - lives in the platform-independent
//! modules and is tested there instead.

use std::collections::HashMap;

use mdns_sd::{Receiver, ServiceDaemon, ServiceEvent, ServiceInfo, TxtProperties};

use super::advertiser::ServiceAdvertiser;
use super::device_info::{DeviceInfo, SERVICE_TYPE};
use super::peer::parse_resolved;
use super::registry::PeerRegistry;

/// Advertises this desktop as a DAnchor service via mDNS.
pub struct MdnsAdvertiser {
    daemon: ServiceDaemon,
    fullname: Option<String>,
}

impl MdnsAdvertiser {
    pub fn new() -> mdns_sd::Result<Self> {
        Ok(Self {
            daemon: ServiceDaemon::new()?,
            fullname: None,
        })
    }
}

impl ServiceAdvertiser for MdnsAdvertiser {
    type Error = mdns_sd::Error;

    fn advertise(
        &mut self,
        instance_name: &str,
        host_name: &str,
        port: u16,
        device: &DeviceInfo,
    ) -> mdns_sd::Result<()> {
        self.withdraw()?;

        // `()` for the IP addresses lets mdns-sd auto-detect every active
        // non-loopback interface, so a USB/RNDIS gadget-mode interface that
        // appears later gets picked up without any extra code here.
        let service_info = ServiceInfo::new(
            SERVICE_TYPE,
            instance_name,
            host_name,
            (),
            port,
            device.to_txt_properties(),
        )?;
        let fullname = service_info.get_fullname().to_string();
        self.daemon.register(service_info)?;
        self.fullname = Some(fullname);
        Ok(())
    }

    fn withdraw(&mut self) -> mdns_sd::Result<()> {
        if let Some(fullname) = self.fullname.take() {
            self.daemon.unregister(&fullname)?;
        }
        Ok(())
    }
}

/// Watches the network for DAnchor peers and feeds resolved/removed events
/// into a `PeerRegistry`.
pub struct MdnsBrowser {
    _daemon: ServiceDaemon,
    receiver: Receiver<ServiceEvent>,
}

impl MdnsBrowser {
    pub fn new() -> mdns_sd::Result<Self> {
        let daemon = ServiceDaemon::new()?;
        let receiver = daemon.browse(SERVICE_TYPE)?;
        Ok(Self {
            _daemon: daemon,
            receiver,
        })
    }

    /// Applies every event queued so far to `registry`, without blocking.
    pub fn drain_into(&self, registry: &mut PeerRegistry) {
        while let Ok(event) = self.receiver.try_recv() {
            apply_event(event, registry);
        }
    }
}

fn apply_event(event: ServiceEvent, registry: &mut PeerRegistry) {
    match event {
        ServiceEvent::ServiceResolved(resolved) => {
            let txt = txt_to_map(&resolved.txt_properties);
            if let Ok(peer) = parse_resolved(
                &resolved.fullname,
                &resolved.host,
                resolved.port,
                resolved.addresses.iter().map(|addr| addr.to_ip_addr()),
                &txt,
            ) {
                registry.found(peer);
            }
        }
        ServiceEvent::ServiceRemoved(_ty_domain, fullname) => {
            registry.removed(&fullname);
        }
        _ => {}
    }
}

fn txt_to_map(txt: &TxtProperties) -> HashMap<String, String> {
    txt.iter()
        .map(|prop| (prop.key().to_string(), prop.val_str().to_string()))
        .collect()
}
