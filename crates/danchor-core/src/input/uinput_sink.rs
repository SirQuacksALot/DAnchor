//! The real `TouchSink`, backed by a `/dev/uinput` virtual touchscreen.
//!
//! This is the one piece of the touch-injection pipeline that isn't unit
//! tested: it needs a real Linux kernel and permission to open
//! `/dev/uinput` (root, or a udev rule granting the `uinput` group), neither
//! of which is available in CI. Keep it as thin as possible - all the
//! logic worth testing lives in `TouchInjector` instead.

use std::collections::HashSet;
use std::io;

use evdev::uinput::VirtualDevice;
use evdev::{
    AbsInfo, AbsoluteAxisCode, AbsoluteAxisEvent, AttributeSet, InputEvent, KeyCode, KeyEvent,
    PropType, UinputAbsSetup,
};

use super::sink::TouchSink;

/// Number of concurrent multi-touch slots the virtual device advertises.
/// `TouchInjector::new` must be constructed with this same value.
pub const MAX_TOUCH_POINTS: usize = 10;

/// `x`/`y` in `TouchEvent` are normalized to the full `u16` range, so the
/// virtual device's position axes are set up to match exactly.
const AXIS_MAX: i32 = u16::MAX as i32;

pub struct UinputTouchSink {
    device: VirtualDevice,
    buffer: Vec<InputEvent>,
    active_slots: HashSet<u16>,
}

impl UinputTouchSink {
    /// Creates and registers the virtual touchscreen device. Requires
    /// permission to open `/dev/uinput`.
    pub fn open() -> io::Result<Self> {
        let mut keys = AttributeSet::<KeyCode>::new();
        keys.insert(KeyCode::BTN_TOUCH);

        let mut props = AttributeSet::<PropType>::new();
        props.insert(PropType::DIRECT);

        let device = VirtualDevice::builder()?
            .name("DAnchor Virtual Touchscreen")
            .with_keys(&keys)?
            .with_properties(&props)?
            .with_absolute_axis(&UinputAbsSetup::new(
                AbsoluteAxisCode::ABS_MT_SLOT,
                AbsInfo::new(0, 0, MAX_TOUCH_POINTS as i32 - 1, 0, 0, 0),
            ))?
            .with_absolute_axis(&UinputAbsSetup::new(
                AbsoluteAxisCode::ABS_MT_TRACKING_ID,
                AbsInfo::new(0, -1, i32::MAX, 0, 0, 0),
            ))?
            .with_absolute_axis(&UinputAbsSetup::new(
                AbsoluteAxisCode::ABS_MT_POSITION_X,
                AbsInfo::new(0, 0, AXIS_MAX, 0, 0, 0),
            ))?
            .with_absolute_axis(&UinputAbsSetup::new(
                AbsoluteAxisCode::ABS_MT_POSITION_Y,
                AbsInfo::new(0, 0, AXIS_MAX, 0, 0, 0),
            ))?
            .with_absolute_axis(&UinputAbsSetup::new(
                AbsoluteAxisCode::ABS_MT_PRESSURE,
                AbsInfo::new(0, 0, 255, 0, 0, 0),
            ))?
            .build()?;

        Ok(Self {
            device,
            buffer: Vec::new(),
            active_slots: HashSet::new(),
        })
    }
}

impl TouchSink for UinputTouchSink {
    type Error = io::Error;

    fn update_slot(
        &mut self,
        slot: u16,
        tracking_id: i32,
        x: u16,
        y: u16,
        pressure: u8,
    ) -> io::Result<()> {
        // BTN_TOUCH tracks "is at least one finger down" - only toggle it on
        // the 0 -> 1 transition, `update_slot` alone can't tell a fresh
        // touch-down from a move of an already-active one.
        if self.active_slots.is_empty() {
            self.buffer
                .push(KeyEvent::new(KeyCode::BTN_TOUCH, 1).into());
        }
        self.active_slots.insert(slot);

        self.buffer
            .push(AbsoluteAxisEvent::new(AbsoluteAxisCode::ABS_MT_SLOT, slot as i32).into());
        self.buffer
            .push(AbsoluteAxisEvent::new(AbsoluteAxisCode::ABS_MT_TRACKING_ID, tracking_id).into());
        self.buffer
            .push(AbsoluteAxisEvent::new(AbsoluteAxisCode::ABS_MT_POSITION_X, x as i32).into());
        self.buffer
            .push(AbsoluteAxisEvent::new(AbsoluteAxisCode::ABS_MT_POSITION_Y, y as i32).into());
        self.buffer.push(
            AbsoluteAxisEvent::new(AbsoluteAxisCode::ABS_MT_PRESSURE, pressure as i32).into(),
        );
        Ok(())
    }

    fn release_slot(&mut self, slot: u16) -> io::Result<()> {
        self.buffer
            .push(AbsoluteAxisEvent::new(AbsoluteAxisCode::ABS_MT_SLOT, slot as i32).into());
        self.buffer
            .push(AbsoluteAxisEvent::new(AbsoluteAxisCode::ABS_MT_TRACKING_ID, -1).into());

        self.active_slots.remove(&slot);
        if self.active_slots.is_empty() {
            self.buffer
                .push(KeyEvent::new(KeyCode::BTN_TOUCH, 0).into());
        }
        Ok(())
    }

    fn sync(&mut self) -> io::Result<()> {
        self.device.emit(&self.buffer)?;
        self.buffer.clear();
        Ok(())
    }
}
