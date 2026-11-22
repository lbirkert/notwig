use std::sync::atomic::{AtomicBool, Ordering};

use crossbeam_queue::ArrayQueue;

/// Maximum number of tracks that can be armed for input at once.
pub const MAX_INPUT_TRACKS: usize = 16;

/// Seconds of input buffered per track. The writer thread drains the recording
/// queue; the engine drains the monitoring queue on every callback.
const INPUT_QUEUE_SECONDS: usize = 2;

struct InputSlot {
    monitor_left: ArrayQueue<f32>,
    monitor_right: ArrayQueue<f32>,
    record_left: ArrayQueue<f32>,
    record_right: ArrayQueue<f32>,
    recording: AtomicBool,
    overflow: AtomicBool,
}

impl InputSlot {
    fn new(capacity: usize) -> Self {
        Self {
            monitor_left: ArrayQueue::new(capacity),
            monitor_right: ArrayQueue::new(capacity),
            record_left: ArrayQueue::new(capacity),
            record_right: ArrayQueue::new(capacity),
            recording: AtomicBool::new(false),
            overflow: AtomicBool::new(false),
        }
    }

    fn clear_record(&self) {
        while self.record_left.pop().is_some() {}
        while self.record_right.pop().is_some() {}
    }
}

fn push_frame(
    left_queue: &ArrayQueue<f32>,
    right_queue: &ArrayQueue<f32>,
    left: f32,
    right: f32,
    overflow: &AtomicBool,
) {
    if left_queue.push(left).is_err() {
        overflow.store(true, Ordering::Relaxed);
        return;
    }
    if right_queue.push(right).is_err() {
        // Keep both channels aligned: remove the left sample we just queued.
        let _ = left_queue.pop();
        overflow.store(true, Ordering::Relaxed);
    }
}

/// Shared input tap between an audio input callback and the engine or recorder.
pub struct InputBus {
    slots: Vec<InputSlot>,
}

impl InputBus {
    #[must_use]
    pub fn new(sample_rate: u32) -> Self {
        let capacity = usize::try_from(sample_rate.max(1))
            .unwrap_or(48_000)
            .saturating_mul(INPUT_QUEUE_SECONDS)
            .max(1024);
        Self {
            slots: (0..MAX_INPUT_TRACKS)
                .map(|_| InputSlot::new(capacity))
                .collect(),
        }
    }

    #[must_use]
    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    /// Push one mono input sample for a slot. The sample is mirrored to both
    /// monitor channels. Called from the audio input callback and therefore
    /// allocation-free.
    pub fn push(&self, slot: u16, sample: f32) {
        self.push_stereo(slot, sample, sample);
    }

    /// Push one stereo input frame for a slot. Called from the audio input
    /// callback and therefore allocation-free.
    pub fn push_stereo(&self, slot: u16, left: f32, right: f32) {
        let Some(slot) = self.slots.get(usize::from(slot)) else {
            return;
        };
        push_frame(
            &slot.monitor_left,
            &slot.monitor_right,
            left,
            right,
            &slot.overflow,
        );
        if slot.recording.load(Ordering::Relaxed) {
            push_frame(
                &slot.record_left,
                &slot.record_right,
                left,
                right,
                &slot.overflow,
            );
        }
    }

    pub fn set_recording(&self, slot: u16, recording: bool) {
        if let Some(slot) = self.slots.get(usize::from(slot)) {
            slot.recording.store(recording, Ordering::Relaxed);
            if !recording {
                slot.clear_record();
            }
        }
    }

    pub fn clear_recording(&self, slot: u16) {
        if let Some(slot) = self.slots.get(usize::from(slot)) {
            slot.clear_record();
        }
    }

    #[must_use]
    pub fn take_overflow(&self, slot: u16) -> bool {
        self.slots
            .get(usize::from(slot))
            .is_some_and(|slot| slot.overflow.swap(false, Ordering::Relaxed))
    }

    pub(crate) fn drain_monitor_into(&self, slot: u16, frames: usize, dest: &mut [f32]) {
        let Some(slot) = self.slots.get(usize::from(slot)) else {
            return;
        };
        for frame in 0..frames.min(dest.len() / 2) {
            let (Some(left), Some(right)) = (slot.monitor_left.pop(), slot.monitor_right.pop())
            else {
                break;
            };
            dest[frame * 2] += left;
            dest[frame * 2 + 1] += right;
        }
    }

    pub fn drain_recording_into(&self, slot: u16, dest: &mut Vec<f32>) -> bool {
        let Some(slot) = self.slots.get(usize::from(slot)) else {
            return false;
        };
        let mut drained = false;
        while let (Some(left), Some(right)) = (slot.record_left.pop(), slot.record_right.pop()) {
            dest.push(left);
            dest.push(right);
            drained = true;
        }
        drained
    }
}

impl std::fmt::Debug for InputBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InputBus")
            .field("slots", &self.slots.len())
            .finish()
    }
}
