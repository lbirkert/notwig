//! Per-device audio monitoring for plugin editor instances.
//!
//! The audio engine owns the plugin instance that processes the live signal,
//! while a plugin's native editor hosts a separate instance. Without a tap the
//! editor instance never sees audio, so meters, analyzers, and some plugin
//! state indicators stay idle.
//!
//! [`AudioMonitor`] is a small lock-free ring of stereo blocks. The audio
//! thread copies a block into the ring; the UI thread drains it and feeds the
//! editor instance. Monitoring is disabled until an editor opens, so the copy
//! only happens when someone is watching.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::DeviceId;
use crossbeam_queue::ArrayQueue;

/// Frames in one monitor block. Large enough for typical live buffers; larger
/// engine blocks are split across several queue entries.
pub const MONITOR_BLOCK_FRAMES: usize = 512;

const MONITOR_BLOCK_SAMPLES: usize = MONITOR_BLOCK_FRAMES * 2;
const MONITOR_QUEUE_BLOCKS: usize = 4;

/// One stereo block copied from the audio thread.
#[derive(Clone, Copy, Debug)]
pub struct MonitorBlock {
    /// Valid frames in `samples`.
    pub frames: usize,
    /// Interleaved stereo samples (`L, R, L, R, …`).
    pub samples: [f32; MONITOR_BLOCK_SAMPLES],
}

impl MonitorBlock {
    fn empty() -> Self {
        Self {
            frames: 0,
            samples: [0.0; MONITOR_BLOCK_SAMPLES],
        }
    }
}

/// Lock-free audio tap for one device.
#[derive(Debug)]
pub struct AudioMonitor {
    enabled: AtomicBool,
    queue: ArrayQueue<MonitorBlock>,
}

impl AudioMonitor {
    /// Create a disabled monitor.
    #[must_use]
    pub fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            queue: ArrayQueue::new(MONITOR_QUEUE_BLOCKS),
        }
    }

    /// Enable or disable copying. Disabling also discards queued blocks.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Release);
        if !enabled {
            while self.queue.pop().is_some() {}
        }
    }

    /// Copy an interleaved stereo block onto the monitor queue.
    ///
    /// Called from the audio thread. This performs no allocation; if the UI
    /// falls behind, the oldest queued block is discarded.
    pub fn push_interleaved(&self, samples: &[f32], frames: usize) {
        if !self.enabled.load(Ordering::Acquire) {
            return;
        }
        let wanted = frames.saturating_mul(2).min(samples.len());
        if wanted == 0 {
            return;
        }
        for chunk in samples[..wanted].chunks(MONITOR_BLOCK_SAMPLES) {
            let block_frames = chunk.len() / 2;
            if block_frames == 0 {
                continue;
            }
            let mut block = MonitorBlock::empty();
            block.frames = block_frames;
            block.samples[..block_frames * 2].copy_from_slice(&chunk[..block_frames * 2]);
            if let Err(block) = self.queue.push(block) {
                let _ = self.queue.pop();
                let _ = self.queue.push(block);
            }
        }
    }

    /// Pop the oldest block. Called from the UI thread.
    #[must_use]
    pub fn pop(&self) -> Option<MonitorBlock> {
        self.queue.pop()
    }
}

impl Default for AudioMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Monitors keyed by device id. The engine creates or reuses one per plugin
/// device; the app looks the same monitor up when opening the editor.
#[derive(Debug, Default)]
pub struct MonitorRegistry {
    monitors: Mutex<BTreeMap<DeviceId, Arc<AudioMonitor>>>,
}

impl MonitorRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the monitor for `device_id`, creating it if needed.
    #[must_use]
    pub fn monitor(&self, device_id: DeviceId) -> Arc<AudioMonitor> {
        let mut monitors = self
            .monitors
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Arc::clone(
            monitors
                .entry(device_id)
                .or_insert_with(|| Arc::new(AudioMonitor::new())),
        )
    }

    /// Return an existing monitor without creating one.
    #[must_use]
    pub fn get(&self, device_id: DeviceId) -> Option<Arc<AudioMonitor>> {
        self.monitors
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&device_id)
            .cloned()
    }

    /// Drop a device's monitor.
    pub fn remove(&self, device_id: DeviceId) {
        self.monitors
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&device_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_monitor_drops_audio() {
        let monitor = AudioMonitor::new();
        monitor.push_interleaved(&[1.0, -1.0, 0.5, -0.5], 2);
        assert!(monitor.pop().is_none());
    }

    #[test]
    fn enabled_monitor_queues_stereo_blocks() {
        let monitor = AudioMonitor::new();
        monitor.set_enabled(true);
        monitor.push_interleaved(&[1.0, -1.0, 0.5, -0.5], 2);
        let block = monitor.pop().expect("block");
        assert_eq!(block.frames, 2);
        assert_eq!(&block.samples[..4], &[1.0, -1.0, 0.5, -0.5]);
    }

    #[test]
    fn registry_reuses_a_monitor() {
        let registry = MonitorRegistry::new();
        let id = DeviceId::from_uuid(uuid::Uuid::from_u128(7));
        let first = registry.monitor(id);
        let second = registry.monitor(id);
        assert!(Arc::ptr_eq(&first, &second));
    }
}
