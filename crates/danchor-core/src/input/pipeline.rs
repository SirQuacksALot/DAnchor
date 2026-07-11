use std::collections::HashMap;

use super::error::InjectError;
use super::sink::TouchSink;
use crate::protocol::{TouchEvent, TouchPhase};

/// Maps incoming wire-protocol `TouchEvent`s onto multi-touch slots and
/// tracking IDs, and drives a `TouchSink` accordingly.
///
/// This holds all the hardware-independent state: which `touch_id` from the
/// wire protocol currently occupies which slot, and which tracking ID it was
/// assigned. It has no knowledge of uinput or any other backend.
pub struct TouchInjector {
    /// `slots[i]` is the wire `touch_id` currently occupying slot `i`, if any.
    slots: Vec<Option<u8>>,
    /// `touch_id` -> `(slot, tracking_id)` for every currently active touch.
    active: HashMap<u8, (u16, i32)>,
    next_tracking_id: i32,
}

impl TouchInjector {
    /// Creates an injector with `max_touch_points` multi-touch slots
    /// available (must match how the sink's device was configured).
    pub fn new(max_touch_points: usize) -> Self {
        assert!(
            max_touch_points > 0 && max_touch_points <= u16::MAX as usize,
            "max_touch_points must be in 1..=u16::MAX"
        );
        Self {
            slots: vec![None; max_touch_points],
            active: HashMap::new(),
            next_tracking_id: 0,
        }
    }

    pub fn max_touch_points(&self) -> usize {
        self.slots.len()
    }

    pub fn active_touch_count(&self) -> usize {
        self.active.len()
    }

    /// Feeds one touch event through to `sink`, allocating/releasing slots
    /// and tracking IDs as needed.
    pub fn apply<S: TouchSink>(
        &mut self,
        sink: &mut S,
        event: &TouchEvent,
    ) -> Result<(), InjectError<S::Error>> {
        match event.phase {
            TouchPhase::Down => self.handle_down(sink, event),
            TouchPhase::Move => self.handle_move(sink, event),
            TouchPhase::Up | TouchPhase::Cancel => self.handle_release(sink, event),
        }
    }

    fn allocate_tracking_id(&mut self) -> i32 {
        let id = self.next_tracking_id;
        // -1 is the uinput sentinel for "slot released", so tracking IDs
        // must stay non-negative; wrap back to 0 rather than go negative on
        // overflow (which would take ~2^31 concurrent touch-downs).
        self.next_tracking_id = if self.next_tracking_id == i32::MAX {
            0
        } else {
            self.next_tracking_id + 1
        };
        id
    }

    fn handle_down<S: TouchSink>(
        &mut self,
        sink: &mut S,
        event: &TouchEvent,
    ) -> Result<(), InjectError<S::Error>> {
        if self.active.contains_key(&event.touch_id) {
            return Err(InjectError::DuplicateTouch {
                touch_id: event.touch_id,
            });
        }

        let slot =
            self.slots
                .iter()
                .position(Option::is_none)
                .ok_or(InjectError::SlotsExhausted {
                    max_touch_points: self.slots.len(),
                })? as u16;

        let tracking_id = self.allocate_tracking_id();
        self.slots[slot as usize] = Some(event.touch_id);
        self.active.insert(event.touch_id, (slot, tracking_id));

        sink.update_slot(slot, tracking_id, event.x, event.y, event.pressure)
            .map_err(InjectError::Sink)?;
        sink.sync().map_err(InjectError::Sink)
    }

    fn handle_move<S: TouchSink>(
        &mut self,
        sink: &mut S,
        event: &TouchEvent,
    ) -> Result<(), InjectError<S::Error>> {
        let &(slot, tracking_id) =
            self.active
                .get(&event.touch_id)
                .ok_or(InjectError::UnknownTouch {
                    touch_id: event.touch_id,
                })?;

        sink.update_slot(slot, tracking_id, event.x, event.y, event.pressure)
            .map_err(InjectError::Sink)?;
        sink.sync().map_err(InjectError::Sink)
    }

    fn handle_release<S: TouchSink>(
        &mut self,
        sink: &mut S,
        event: &TouchEvent,
    ) -> Result<(), InjectError<S::Error>> {
        let (slot, _tracking_id) =
            self.active
                .remove(&event.touch_id)
                .ok_or(InjectError::UnknownTouch {
                    touch_id: event.touch_id,
                })?;
        self.slots[slot as usize] = None;

        sink.release_slot(slot).map_err(InjectError::Sink)?;
        sink.sync().map_err(InjectError::Sink)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum SinkCall {
        Update {
            slot: u16,
            tracking_id: i32,
            x: u16,
            y: u16,
            pressure: u8,
        },
        Release {
            slot: u16,
        },
        Sync,
    }

    #[derive(Default)]
    struct MockSink {
        calls: Vec<SinkCall>,
    }

    impl TouchSink for MockSink {
        type Error = std::convert::Infallible;

        fn update_slot(
            &mut self,
            slot: u16,
            tracking_id: i32,
            x: u16,
            y: u16,
            pressure: u8,
        ) -> Result<(), Self::Error> {
            self.calls.push(SinkCall::Update {
                slot,
                tracking_id,
                x,
                y,
                pressure,
            });
            Ok(())
        }

        fn release_slot(&mut self, slot: u16) -> Result<(), Self::Error> {
            self.calls.push(SinkCall::Release { slot });
            Ok(())
        }

        fn sync(&mut self) -> Result<(), Self::Error> {
            self.calls.push(SinkCall::Sync);
            Ok(())
        }
    }

    fn touch(touch_id: u8, phase: TouchPhase, x: u16, y: u16) -> TouchEvent {
        TouchEvent {
            touch_id,
            phase,
            x,
            y,
            pressure: 128,
            timestamp_ms: 0,
        }
    }

    #[test]
    fn down_allocates_slot_zero_and_syncs() {
        let mut injector = TouchInjector::new(4);
        let mut sink = MockSink::default();

        injector
            .apply(&mut sink, &touch(7, TouchPhase::Down, 10, 20))
            .unwrap();

        assert_eq!(
            sink.calls,
            vec![
                SinkCall::Update {
                    slot: 0,
                    tracking_id: 0,
                    x: 10,
                    y: 20,
                    pressure: 128
                },
                SinkCall::Sync,
            ]
        );
        assert_eq!(injector.active_touch_count(), 1);
    }

    #[test]
    fn move_reuses_the_slot_and_tracking_id_from_down() {
        let mut injector = TouchInjector::new(4);
        let mut sink = MockSink::default();

        injector
            .apply(&mut sink, &touch(7, TouchPhase::Down, 10, 20))
            .unwrap();
        injector
            .apply(&mut sink, &touch(7, TouchPhase::Move, 11, 21))
            .unwrap();

        assert_eq!(
            sink.calls[2],
            SinkCall::Update {
                slot: 0,
                tracking_id: 0,
                x: 11,
                y: 21,
                pressure: 128
            }
        );
    }

    #[test]
    fn up_releases_the_slot_which_can_then_be_reused() {
        let mut injector = TouchInjector::new(1);
        let mut sink = MockSink::default();

        injector
            .apply(&mut sink, &touch(1, TouchPhase::Down, 0, 0))
            .unwrap();
        injector
            .apply(&mut sink, &touch(1, TouchPhase::Up, 0, 0))
            .unwrap();

        assert_eq!(sink.calls[2], SinkCall::Release { slot: 0 });
        assert_eq!(injector.active_touch_count(), 0);

        // The single slot should be free again for a new touch.
        injector
            .apply(&mut sink, &touch(2, TouchPhase::Down, 5, 5))
            .unwrap();
        assert_eq!(
            sink.calls[4],
            SinkCall::Update {
                slot: 0,
                tracking_id: 1,
                x: 5,
                y: 5,
                pressure: 128
            }
        );
    }

    #[test]
    fn cancel_releases_the_slot_like_up() {
        let mut injector = TouchInjector::new(2);
        let mut sink = MockSink::default();

        injector
            .apply(&mut sink, &touch(1, TouchPhase::Down, 0, 0))
            .unwrap();
        injector
            .apply(&mut sink, &touch(1, TouchPhase::Cancel, 0, 0))
            .unwrap();

        assert_eq!(sink.calls[2], SinkCall::Release { slot: 0 });
        assert_eq!(injector.active_touch_count(), 0);
    }

    #[test]
    fn concurrent_touches_get_distinct_slots_and_tracking_ids() {
        let mut injector = TouchInjector::new(4);
        let mut sink = MockSink::default();

        injector
            .apply(&mut sink, &touch(1, TouchPhase::Down, 0, 0))
            .unwrap();
        injector
            .apply(&mut sink, &touch(2, TouchPhase::Down, 100, 100))
            .unwrap();

        assert_eq!(
            sink.calls[0],
            SinkCall::Update {
                slot: 0,
                tracking_id: 0,
                x: 0,
                y: 0,
                pressure: 128
            }
        );
        assert_eq!(
            sink.calls[2],
            SinkCall::Update {
                slot: 1,
                tracking_id: 1,
                x: 100,
                y: 100,
                pressure: 128
            }
        );
    }

    #[test]
    fn down_for_already_active_touch_id_is_rejected() {
        let mut injector = TouchInjector::new(4);
        let mut sink = MockSink::default();

        injector
            .apply(&mut sink, &touch(1, TouchPhase::Down, 0, 0))
            .unwrap();
        let err = injector
            .apply(&mut sink, &touch(1, TouchPhase::Down, 1, 1))
            .unwrap_err();

        assert_eq!(err, InjectError::DuplicateTouch { touch_id: 1 });
    }

    #[test]
    fn move_for_unknown_touch_id_is_rejected() {
        let mut injector = TouchInjector::new(4);
        let mut sink = MockSink::default();

        let err = injector
            .apply(&mut sink, &touch(9, TouchPhase::Move, 0, 0))
            .unwrap_err();

        assert_eq!(err, InjectError::UnknownTouch { touch_id: 9 });
    }

    #[test]
    fn up_for_unknown_touch_id_is_rejected() {
        let mut injector = TouchInjector::new(4);
        let mut sink = MockSink::default();

        let err = injector
            .apply(&mut sink, &touch(9, TouchPhase::Up, 0, 0))
            .unwrap_err();

        assert_eq!(err, InjectError::UnknownTouch { touch_id: 9 });
    }

    #[test]
    fn down_beyond_capacity_is_rejected_without_touching_the_sink() {
        let mut injector = TouchInjector::new(1);
        let mut sink = MockSink::default();

        injector
            .apply(&mut sink, &touch(1, TouchPhase::Down, 0, 0))
            .unwrap();
        let err = injector
            .apply(&mut sink, &touch(2, TouchPhase::Down, 0, 0))
            .unwrap_err();

        assert_eq!(
            err,
            InjectError::SlotsExhausted {
                max_touch_points: 1
            }
        );
        // Only the first Down's Update+Sync should have reached the sink.
        assert_eq!(sink.calls.len(), 2);
    }

    #[test]
    fn released_slot_gets_a_fresh_tracking_id_on_reuse() {
        let mut injector = TouchInjector::new(1);
        let mut sink = MockSink::default();

        injector
            .apply(&mut sink, &touch(1, TouchPhase::Down, 0, 0))
            .unwrap();
        injector
            .apply(&mut sink, &touch(1, TouchPhase::Up, 0, 0))
            .unwrap();
        injector
            .apply(&mut sink, &touch(1, TouchPhase::Down, 0, 0))
            .unwrap();

        let SinkCall::Update {
            tracking_id: first, ..
        } = sink.calls[0]
        else {
            panic!("expected an Update call");
        };
        let SinkCall::Update {
            tracking_id: second,
            ..
        } = sink.calls[4]
        else {
            panic!("expected an Update call");
        };
        assert_ne!(first, second);
    }
}
