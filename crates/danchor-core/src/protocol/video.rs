use super::error::ProtocolError;
use std::collections::BTreeMap;

/// One wire-sized slice of an encoded video frame. A frame is almost always
/// larger than a single safe UDP datagram, so it travels as a run of
/// fragments sharing `frame_id`, each tagged with its position in the run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoFragment {
    pub frame_id: u32,
    pub fragment_index: u16,
    pub fragment_count: u16,
    pub keyframe: bool,
    pub data: Vec<u8>,
}

impl VideoFragment {
    const HEADER_LEN: usize = 4 + 2 + 2 + 1;

    pub(crate) fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.frame_id.to_be_bytes());
        out.extend_from_slice(&self.fragment_index.to_be_bytes());
        out.extend_from_slice(&self.fragment_count.to_be_bytes());
        out.push(self.keyframe as u8);
        out.extend_from_slice(&self.data);
    }

    pub(crate) fn decode(buf: &[u8]) -> Result<Self, ProtocolError> {
        if buf.len() < Self::HEADER_LEN {
            return Err(ProtocolError::BufferTooShort {
                expected: Self::HEADER_LEN,
                actual: buf.len(),
            });
        }

        let frame_id = u32::from_be_bytes(buf[0..4].try_into().unwrap());
        let fragment_index = u16::from_be_bytes(buf[4..6].try_into().unwrap());
        let fragment_count = u16::from_be_bytes(buf[6..8].try_into().unwrap());
        let keyframe = buf[8] != 0;

        if fragment_count == 0 || fragment_index >= fragment_count {
            return Err(ProtocolError::FragmentIndexOutOfRange {
                index: fragment_index,
                count: fragment_count,
            });
        }

        Ok(Self {
            frame_id,
            fragment_index,
            fragment_count,
            keyframe,
            data: buf[Self::HEADER_LEN..].to_vec(),
        })
    }
}

/// Splits one encoded video frame into fragments no larger than
/// `max_fragment_size` bytes of payload each.
pub fn fragment_frame(
    frame_id: u32,
    data: &[u8],
    keyframe: bool,
    max_fragment_size: usize,
) -> Result<Vec<VideoFragment>, ProtocolError> {
    assert!(max_fragment_size > 0, "max_fragment_size must be positive");

    let chunks: Vec<&[u8]> = if data.is_empty() {
        vec![&[][..]]
    } else {
        data.chunks(max_fragment_size).collect()
    };

    if chunks.len() > u16::MAX as usize {
        return Err(ProtocolError::TooManyFragments {
            count: chunks.len(),
            max: u16::MAX,
        });
    }

    let fragment_count = chunks.len() as u16;
    Ok(chunks
        .into_iter()
        .enumerate()
        .map(|(index, chunk)| VideoFragment {
            frame_id,
            fragment_index: index as u16,
            fragment_count,
            keyframe,
            data: chunk.to_vec(),
        })
        .collect())
}

/// A fully reassembled encoded video frame, ready to hand to the decoder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteFrame {
    pub frame_id: u32,
    pub keyframe: bool,
    pub data: Vec<u8>,
}

struct PartialFrame {
    received_count: u16,
    keyframe: bool,
    fragments: Vec<Option<Vec<u8>>>,
}

impl PartialFrame {
    fn new(fragment_count: u16, keyframe: bool) -> Self {
        Self {
            received_count: 0,
            keyframe,
            fragments: vec![None; fragment_count as usize],
        }
    }
}

/// Reassembles fragmented video frames as `VideoFragment`s arrive, possibly
/// out of order or with duplicates.
///
/// Bounded to `max_pending_frames` in-flight frames: once at capacity, the
/// oldest incomplete frame is dropped to make room for a new one. On a
/// real-time stream that's the right trade-off - a frame stalled on one lost
/// fragment is stale anyway, and the decoder recovers at the next keyframe
/// rather than the pipeline stalling on it indefinitely.
pub struct FrameReassembler {
    max_pending_frames: usize,
    pending: BTreeMap<u32, PartialFrame>,
}

impl FrameReassembler {
    pub fn new(max_pending_frames: usize) -> Self {
        assert!(
            max_pending_frames > 0,
            "max_pending_frames must be positive"
        );
        Self {
            max_pending_frames,
            pending: BTreeMap::new(),
        }
    }

    pub fn pending_frame_count(&self) -> usize {
        self.pending.len()
    }

    /// Feeds one fragment in. Returns the reassembled frame once every
    /// fragment for its `frame_id` has arrived.
    pub fn insert(&mut self, fragment: VideoFragment) -> Option<CompleteFrame> {
        let VideoFragment {
            frame_id,
            fragment_index,
            fragment_count,
            keyframe,
            data,
        } = fragment;

        if !self.pending.contains_key(&frame_id) {
            if self.pending.len() >= self.max_pending_frames
                && let Some(&oldest_id) = self.pending.keys().next()
            {
                self.pending.remove(&oldest_id);
            }
            self.pending
                .insert(frame_id, PartialFrame::new(fragment_count, keyframe));
        }

        let partial = self.pending.get_mut(&frame_id).unwrap();
        let slot = partial.fragments.get_mut(fragment_index as usize)?;
        if slot.is_some() {
            return None; // duplicate fragment
        }
        *slot = Some(data);
        partial.received_count += 1;

        if (partial.received_count as usize) < partial.fragments.len() {
            return None;
        }

        let PartialFrame {
            keyframe,
            fragments,
            ..
        } = self.pending.remove(&frame_id).unwrap();

        let mut data = Vec::new();
        for chunk in fragments {
            data.extend_from_slice(
                &chunk.expect("all fragments present once received_count matches"),
            );
        }

        Some(CompleteFrame {
            frame_id,
            keyframe,
            data,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_header_round_trips() {
        let fragment = VideoFragment {
            frame_id: 42,
            fragment_index: 1,
            fragment_count: 3,
            keyframe: true,
            data: vec![1, 2, 3, 4, 5],
        };
        let mut buf = Vec::new();
        fragment.encode(&mut buf);
        assert_eq!(VideoFragment::decode(&buf).unwrap(), fragment);
    }

    #[test]
    fn fragment_frame_splits_evenly() {
        let data = vec![0u8; 10];
        let fragments = fragment_frame(1, &data, false, 4).unwrap();
        assert_eq!(fragments.len(), 3);
        assert_eq!(fragments[0].data.len(), 4);
        assert_eq!(fragments[1].data.len(), 4);
        assert_eq!(fragments[2].data.len(), 2);
        assert!(fragments.iter().all(|f| f.fragment_count == 3));
    }

    #[test]
    fn fragment_frame_handles_empty_data() {
        let fragments = fragment_frame(1, &[], true, 4).unwrap();
        assert_eq!(fragments.len(), 1);
        assert_eq!(fragments[0].fragment_count, 1);
        assert!(fragments[0].data.is_empty());
    }

    #[test]
    fn decode_rejects_zero_fragment_count() {
        let fragment = VideoFragment {
            frame_id: 1,
            fragment_index: 0,
            fragment_count: 0,
            keyframe: false,
            data: vec![],
        };
        let mut buf = Vec::new();
        fragment.encode(&mut buf);
        let err = VideoFragment::decode(&buf).unwrap_err();
        assert_eq!(
            err,
            ProtocolError::FragmentIndexOutOfRange { index: 0, count: 0 }
        );
    }

    #[test]
    fn reassembles_in_order_fragments() {
        let data = (0u8..50).collect::<Vec<_>>();
        let fragments = fragment_frame(7, &data, true, 8).unwrap();

        let mut reassembler = FrameReassembler::new(4);
        let mut complete = None;
        for fragment in fragments {
            complete = reassembler.insert(fragment);
        }

        let complete = complete.expect("frame should be complete after last fragment");
        assert_eq!(complete.frame_id, 7);
        assert!(complete.keyframe);
        assert_eq!(complete.data, data);
        assert_eq!(reassembler.pending_frame_count(), 0);
    }

    #[test]
    fn reassembles_out_of_order_fragments() {
        let data = (0u8..50).collect::<Vec<_>>();
        let mut fragments = fragment_frame(7, &data, false, 8).unwrap();
        fragments.reverse();

        let mut reassembler = FrameReassembler::new(4);
        let mut complete = None;
        for fragment in fragments {
            complete = reassembler.insert(fragment);
        }

        assert_eq!(complete.unwrap().data, data);
    }

    #[test]
    fn duplicate_fragment_is_ignored() {
        let data = vec![1, 2, 3, 4];
        let fragments = fragment_frame(1, &data, false, 2).unwrap();
        assert_eq!(fragments.len(), 2);

        let mut reassembler = FrameReassembler::new(4);
        assert!(reassembler.insert(fragments[0].clone()).is_none());
        // Re-deliver the first fragment (e.g. a spurious UDP retransmit).
        assert!(reassembler.insert(fragments[0].clone()).is_none());
        let complete = reassembler.insert(fragments[1].clone());
        assert_eq!(complete.unwrap().data, data);
    }

    #[test]
    fn incomplete_frame_stays_pending() {
        let data = vec![1, 2, 3, 4, 5, 6];
        let fragments = fragment_frame(1, &data, false, 2).unwrap();
        assert_eq!(fragments.len(), 3);

        let mut reassembler = FrameReassembler::new(4);
        assert!(reassembler.insert(fragments[0].clone()).is_none());
        assert!(reassembler.insert(fragments[1].clone()).is_none());
        assert_eq!(reassembler.pending_frame_count(), 1);
    }

    #[test]
    fn interleaved_frames_reassemble_independently() {
        let data_a = vec![1u8; 6];
        let data_b = vec![2u8; 6];
        let frags_a = fragment_frame(1, &data_a, false, 2).unwrap();
        let frags_b = fragment_frame(2, &data_b, false, 2).unwrap();

        let mut reassembler = FrameReassembler::new(4);
        assert!(reassembler.insert(frags_a[0].clone()).is_none());
        assert!(reassembler.insert(frags_b[0].clone()).is_none());
        assert!(reassembler.insert(frags_a[1].clone()).is_none());
        assert!(reassembler.insert(frags_b[1].clone()).is_none());
        let complete_a = reassembler.insert(frags_a[2].clone()).unwrap();
        let complete_b = reassembler.insert(frags_b[2].clone()).unwrap();

        assert_eq!(complete_a.data, data_a);
        assert_eq!(complete_b.data, data_b);
    }

    #[test]
    fn evicts_oldest_pending_frame_when_at_capacity() {
        let mut reassembler = FrameReassembler::new(2);

        // Start three frames, each missing their final fragment, over a
        // reassembler that only has room for two in flight.
        for frame_id in 1..=3u32 {
            let data = vec![frame_id as u8; 4];
            let fragments = fragment_frame(frame_id, &data, false, 2).unwrap();
            assert!(reassembler.insert(fragments[0].clone()).is_none());
        }

        // Frame 1 should have been evicted to make room for frame 3.
        assert_eq!(reassembler.pending_frame_count(), 2);

        let data_1 = vec![1u8; 4];
        let fragments_1 = fragment_frame(1, &data_1, false, 2).unwrap();
        // Completing frame 1 now starts a *new* partial frame rather than
        // resuming the evicted one, so it won't complete from one fragment.
        assert!(reassembler.insert(fragments_1[1].clone()).is_none());
    }

    #[test]
    fn too_many_fragments_is_rejected() {
        let data = vec![0u8; u16::MAX as usize + 1];
        let err = fragment_frame(1, &data, false, 1).unwrap_err();
        assert_eq!(
            err,
            ProtocolError::TooManyFragments {
                count: u16::MAX as usize + 1,
                max: u16::MAX,
            }
        );
    }
}
