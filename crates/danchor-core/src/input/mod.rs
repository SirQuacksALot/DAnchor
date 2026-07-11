//! Touch-input injection: turns wire-protocol `TouchEvent`s into a virtual
//! Linux multi-touch device via uinput.
//!
//! The OS boundary (`TouchSink`) and everything above it (`TouchInjector`)
//! are platform-independent and unit tested. Only the concrete uinput
//! backend is Linux-only and untestable in CI.

mod error;
mod pipeline;
mod sink;

pub use error::InjectError;
pub use pipeline::TouchInjector;
pub use sink::TouchSink;

#[cfg(target_os = "linux")]
mod uinput_sink;

#[cfg(target_os = "linux")]
pub use uinput_sink::{MAX_TOUCH_POINTS, UinputTouchSink};
