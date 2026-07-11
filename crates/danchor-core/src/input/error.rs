use std::fmt;

/// Errors from feeding a `TouchEvent` through a `TouchInjector`.
///
/// `E` is the sink's own error type (e.g. `std::io::Error` for the real
/// uinput backend), kept generic so the pipeline doesn't force one error
/// type onto every possible sink implementation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InjectError<E> {
    /// Every multitouch slot is already occupied; the tablet reported more
    /// simultaneous touch points than this injector was configured for.
    SlotsExhausted { max_touch_points: usize },
    /// A `Move`, `Up`, or `Cancel` referenced a `touch_id` that was never
    /// started with `Down` (or was already released).
    UnknownTouch { touch_id: u8 },
    /// A `Down` was received for a `touch_id` that's already active.
    DuplicateTouch { touch_id: u8 },
    /// The underlying sink (e.g. the real uinput device) failed.
    Sink(E),
}

impl<E: fmt::Display> fmt::Display for InjectError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SlotsExhausted { max_touch_points } => write!(
                f,
                "all {max_touch_points} multi-touch slots are already in use"
            ),
            Self::UnknownTouch { touch_id } => {
                write!(f, "touch_id {touch_id} has no active touch to update")
            }
            Self::DuplicateTouch { touch_id } => {
                write!(f, "touch_id {touch_id} already has an active touch")
            }
            Self::Sink(err) => write!(f, "touch sink error: {err}"),
        }
    }
}

impl<E: fmt::Debug + fmt::Display> std::error::Error for InjectError<E> {}
