use super::error::ProtocolError;

/// Phase of a single touch point in a multi-touch gesture, mirroring the
/// down/move/up/cancel lifecycle uinput needs on the receiving end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchPhase {
    Down,
    Move,
    Up,
    Cancel,
}

impl TouchPhase {
    fn to_u8(self) -> u8 {
        match self {
            Self::Down => 0,
            Self::Move => 1,
            Self::Up => 2,
            Self::Cancel => 3,
        }
    }

    fn from_u8(v: u8) -> Result<Self, ProtocolError> {
        match v {
            0 => Ok(Self::Down),
            1 => Ok(Self::Move),
            2 => Ok(Self::Up),
            3 => Ok(Self::Cancel),
            other => Err(ProtocolError::UnknownTouchPhase(other)),
        }
    }
}

/// A single touch-point update, sent client -> desktop as the input
/// back-channel. `x`/`y` are normalized to the full `u16` range (0 = edge,
/// 65535 = opposite edge) so the payload doesn't need to know the tablet's
/// physical resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TouchEvent {
    pub touch_id: u8,
    pub phase: TouchPhase,
    pub x: u16,
    pub y: u16,
    pub pressure: u8,
    pub timestamp_ms: u64,
}

impl TouchEvent {
    pub(crate) const ENCODED_LEN: usize = 1 + 1 + 2 + 2 + 1 + 8;

    pub(crate) fn encode(&self, out: &mut Vec<u8>) {
        out.push(self.touch_id);
        out.push(self.phase.to_u8());
        out.extend_from_slice(&self.x.to_be_bytes());
        out.extend_from_slice(&self.y.to_be_bytes());
        out.push(self.pressure);
        out.extend_from_slice(&self.timestamp_ms.to_be_bytes());
    }

    pub(crate) fn decode(buf: &[u8]) -> Result<Self, ProtocolError> {
        if buf.len() != Self::ENCODED_LEN {
            return Err(ProtocolError::PayloadLengthMismatch {
                declared: Self::ENCODED_LEN,
                actual: buf.len(),
            });
        }

        Ok(Self {
            touch_id: buf[0],
            phase: TouchPhase::from_u8(buf[1])?,
            x: u16::from_be_bytes(buf[2..4].try_into().unwrap()),
            y: u16::from_be_bytes(buf[4..6].try_into().unwrap()),
            pressure: buf[6],
            timestamp_ms: u64::from_be_bytes(buf[7..15].try_into().unwrap()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> TouchEvent {
        TouchEvent {
            touch_id: 3,
            phase: TouchPhase::Move,
            x: 40000,
            y: 12345,
            pressure: 200,
            timestamp_ms: 0x0102_0304_0506,
        }
    }

    #[test]
    fn round_trips() {
        let event = sample();
        let mut buf = Vec::new();
        event.encode(&mut buf);
        assert_eq!(buf.len(), TouchEvent::ENCODED_LEN);

        let decoded = TouchEvent::decode(&buf).unwrap();
        assert_eq!(decoded, event);
    }

    #[test]
    fn all_phases_round_trip() {
        for phase in [
            TouchPhase::Down,
            TouchPhase::Move,
            TouchPhase::Up,
            TouchPhase::Cancel,
        ] {
            let event = TouchEvent { phase, ..sample() };
            let mut buf = Vec::new();
            event.encode(&mut buf);
            assert_eq!(TouchEvent::decode(&buf).unwrap(), event);
        }
    }

    #[test]
    fn rejects_wrong_length() {
        let err = TouchEvent::decode(&[0; 5]).unwrap_err();
        assert_eq!(
            err,
            ProtocolError::PayloadLengthMismatch {
                declared: TouchEvent::ENCODED_LEN,
                actual: 5
            }
        );
    }

    #[test]
    fn rejects_unknown_phase() {
        let mut buf = Vec::new();
        sample().encode(&mut buf);
        buf[1] = 0xff;
        let err = TouchEvent::decode(&buf).unwrap_err();
        assert_eq!(err, ProtocolError::UnknownTouchPhase(0xff));
    }
}
