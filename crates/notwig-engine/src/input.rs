use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

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
    sample_rate: u32,
    input_latency_samples: AtomicU64,
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
            sample_rate: sample_rate.max(1),
            input_latency_samples: AtomicU64::new(0),
            slots: (0..MAX_INPUT_TRACKS)
                .map(|_| InputSlot::new(capacity))
                .collect(),
        }
    }

    #[must_use]
    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    pub fn set_input_latency_samples(&self, samples: u64) {
        self.input_latency_samples.store(samples, Ordering::Relaxed);
    }

    #[must_use]
    pub fn input_latency_samples(&self) -> u64 {
        self.input_latency_samples.load(Ordering::Relaxed)
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

    pub(crate) fn discard_monitor(&self, slot: u16, frames: usize) {
        let Some(slot) = self.slots.get(usize::from(slot)) else {
            return;
        };
        for _ in 0..frames {
            if slot.monitor_left.pop().is_none() {
                break;
            }
            let _ = slot.monitor_right.pop();
        }
    }

    pub(crate) fn drain_monitor_into(&self, slot: u16, frames: usize, dest: &mut [f32]) {
        let Some(slot) = self.slots.get(usize::from(slot)) else {
            return;
        };
        // Monitoring must not accumulate unbounded latency if input and output
        // clocks drift or one callback path stalls. Keep at most two requested
        // blocks or 5 ms, whichever is larger.
        let max_backlog = frames
            .saturating_mul(2)
            .max(usize::try_from(self.sample_rate / 200).unwrap_or(240));
        let queued = slot.monitor_left.len();
        if queued > max_backlog {
            for _ in 0..queued - max_backlog {
                let _ = slot.monitor_left.pop();
                let _ = slot.monitor_right.pop();
            }
        }
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
            .field("sample_rate", &self.sample_rate)
            .field("input_latency_samples", &self.input_latency_samples())
            .field("slots", &self.slots.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_latency_is_published() {
        let bus = InputBus::new(48_000);
        assert_eq!(bus.input_latency_samples(), 0);
        bus.set_input_latency_samples(384);
        assert_eq!(bus.input_latency_samples(), 384);
    }

    #[test]
    fn monitor_drain_drops_stale_backlog_to_bound_latency() {
        let bus = InputBus::new(48_000);
        for index in 0..5_000 {
            bus.push(0, index as f32);
        }
        let mut output = vec![0.0_f32; 128 * 2];
        bus.drain_monitor_into(0, 128, &mut output);
        assert_eq!(output[0], 4_744.0);
        assert_eq!(output[1], 4_744.0);
    }
}
