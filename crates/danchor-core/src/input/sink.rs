/// Abstraction over "a receiver of synthesized multi-touch input".
///
/// This is the one OS/hardware boundary in the touch-injection pipeline: the
/// real implementation talks to `/dev/uinput`, but `TouchInjector`'s
/// slot-allocation and protocol logic only depends on this trait, so it can
/// be unit tested against a mock without any real device.
///
/// A slot mirrors the Linux multi-touch protocol B model: each concurrent
/// touch point owns a `slot` for its lifetime, identified within that slot
/// by a `tracking_id` that's unique among currently-active touches.
pub trait TouchSink {
    type Error;

    /// Reports the touch point in `slot` as present at `(x, y)` with the
    /// given pressure, tagged with `tracking_id`.
    fn update_slot(
        &mut self,
        slot: u16,
        tracking_id: i32,
        x: u16,
        y: u16,
        pressure: u8,
    ) -> Result<(), Self::Error>;

    /// Ends the touch point in `slot`.
    fn release_slot(&mut self, slot: u16) -> Result<(), Self::Error>;

    /// Commits every `update_slot`/`release_slot` call since the last
    /// `sync` as one atomic input frame.
    fn sync(&mut self) -> Result<(), Self::Error>;
}
