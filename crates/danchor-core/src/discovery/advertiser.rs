use super::device_info::DeviceInfo;

/// Abstraction over "publish this desktop as discoverable on the network".
///
/// The one OS/network boundary in the advertising half of discovery: the
/// real implementation opens multicast sockets via mDNS, but nothing else
/// in this crate depends on that, so callers can be tested against a mock.
pub trait ServiceAdvertiser {
    type Error;

    /// Starts advertising this device as `instance_name`, resolvable at
    /// `host_name` on `port`. Replaces any advertisement already in
    /// progress.
    fn advertise(
        &mut self,
        instance_name: &str,
        host_name: &str,
        port: u16,
        device: &DeviceInfo,
    ) -> Result<(), Self::Error>;

    /// Stops advertising. A no-op if nothing is currently advertised.
    fn withdraw(&mut self) -> Result<(), Self::Error>;
}
