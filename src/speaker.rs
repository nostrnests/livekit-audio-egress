use std::collections::VecDeque;
use crate::mixer::MixerData;

pub struct SpeakerChannel {
    sid: String,
    pts: i64,
    frames: VecDeque<MixerData>
}

impl SpeakerChannel {
    pub fn new(sid: String) -> Self {
        Self {
            sid,
            pts: 0,
            frames: VecDeque::new()
        }
    }

    pub fn put(&mut self, data: MixerData) {
        self.frames.push_back(data);
    }

    /// Get samples for the next frame,
    /// if no samples are buffered silence (None) will be returned
    pub fn next_samples(&mut self, next_pts: i64) -> Option<Vec<i16>> {
        None
    }
}