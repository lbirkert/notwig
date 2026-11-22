use std::sync::atomic::{AtomicBool, Ordering};

use crossbeam_queue::ArrayQueue;

/// Maximum number of tracks that can be armed for input at once.
pub const MAX_INPUT_TRACKS: usize = 16;

/// Seconds of input buffered per track. The writer thread drains the recording
/// queue; the engine drains the monitoring queue on every callback.
const INPUT_QUEUE_SECONDS: usize = 2;

struct InputSlot {
    monitor: ArrayQueue<f32>,
    record: ArrayQueue<f32>,
    recording: AtomicBool,
    overflow: AtomicBool,
}

impl InputSlot {
    fn new(capacity: usize) -> Self {
        Self {
            monitor: ArrayQueue::new(capacity),
            record: ArrayQueue::new(capacity),
            recording: AtomicBool::new(false),
            overflow: AtomicBool::new(false),
        }
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

    /// Push one selected input sample for a slot. Called from the audio input
    /// callback and therefore allocation-free.
    pub fn push(&self, slot: u16, sample: f32) {
        let Some(slot) = self.slots.get(usize::from(slot)) else {
            return;
        };
        if slot.monitor.push(sample).is_err() {
            slot.overflow.store(true, Ordering::Relaxed);
        }
        if slot.recording.load(Ordering::Relaxed) && slot.record.push(sample).is_err() {
            slot.overflow.store(true, Ordering::Relaxed);
        }
    }

    pub fn set_recording(&self, slot: u16, recording: bool) {
        if let Some(slot) = self.slots.get(usize::from(slot)) {
            slot.recording.store(recording, Ordering::Relaxed);
            if !recording {
                while slot.record.pop().is_some() {}
            }
        }
    }

    pub fn clear_recording(&self, slot: u16) {
        if let Some(slot) = self.slots.get(usize::from(slot)) {
            while slot.record.pop().is_some() {}
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
            let Some(sample) = slot.monitor.pop() else {
                break;
            };
            dest[frame * 2] += sample;
            dest[frame * 2 + 1] += sample;
        }
    }

    pub fn drain_recording_into(&self, slot: u16, dest: &mut Vec<f32>) -> bool {
        let Some(slot) = self.slots.get(usize::from(slot)) else {
            return false;
        };
        let mut drained = false;
        while let Some(sample) = slot.record.pop() {
            dest.push(sample);
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
