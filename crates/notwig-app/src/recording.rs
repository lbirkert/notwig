use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
#[cfg(not(target_arch = "wasm32"))]
use std::thread::{self, JoinHandle};
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

use notwig_core::{AudioAssetBuffer, TrackId};
use notwig_engine::InputBus;

pub(crate) struct RecordingSession {
    bus: Arc<InputBus>,
    slots: Vec<(TrackId, u16)>,
    sample_rate: u32,
    running: Arc<AtomicBool>,
    buffers: Arc<Mutex<BTreeMap<TrackId, Vec<f32>>>>,
    overflow: Arc<AtomicBool>,
    #[cfg(not(target_arch = "wasm32"))]
    thread: Option<JoinHandle<()>>,
}

impl RecordingSession {
    pub(crate) fn start(bus: Arc<InputBus>, slots: Vec<(TrackId, u16)>, sample_rate: u32) -> Self {
        for (_, slot) in &slots {
            bus.clear_recording(*slot);
            bus.set_recording(*slot, true);
        }
        let running = Arc::new(AtomicBool::new(true));
        let buffers = Arc::new(Mutex::new(BTreeMap::new()));
        let overflow = Arc::new(AtomicBool::new(false));
        #[cfg(not(target_arch = "wasm32"))]
        let thread = {
            let running = Arc::clone(&running);
            let buffers = Arc::clone(&buffers);
            let overflow = Arc::clone(&overflow);
            let bus = Arc::clone(&bus);
            let slots = slots.clone();
            thread::Builder::new()
                .name("notwig-recording-writer".into())
                .spawn(move || {
                    while running.load(Ordering::Relaxed) {
                        drain(&bus, &slots, &buffers, &overflow);
                        thread::sleep(Duration::from_millis(2));
                    }
                    drain(&bus, &slots, &buffers, &overflow);
                })
                .ok()
        };
        Self {
            bus,
            slots,
            sample_rate,
            running,
            buffers,
            overflow,
            #[cfg(not(target_arch = "wasm32"))]
            thread,
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn poll(&self) {
        drain(&self.bus, &self.slots, &self.buffers, &self.overflow);
    }

    #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
    pub(crate) fn finish(mut self) -> Result<BTreeMap<TrackId, AudioAssetBuffer>, String> {
        self.running.store(false, Ordering::Relaxed);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        drain(&self.bus, &self.slots, &self.buffers, &self.overflow);
        for (_, slot) in &self.slots {
            self.bus.set_recording(*slot, false);
        }
        if self.overflow.load(Ordering::Relaxed) {
            return Err("recording input overflowed; the take was discarded".into());
        }
        let buffers = self
            .buffers
            .lock()
            .map_err(|_| "recording buffer lock poisoned".to_string())?
            .clone();
        let mut captures = BTreeMap::new();
        for (track_id, samples) in buffers {
            if samples.is_empty() {
                continue;
            }
            captures.insert(
                track_id,
                AudioAssetBuffer {
                    sample_rate: self.sample_rate,
                    channels: 2,
                    samples,
                },
            );
        }
        if captures.is_empty() {
            return Err("recording produced no audio".into());
        }
        Ok(captures)
    }
}

fn drain(
    bus: &InputBus,
    slots: &[(TrackId, u16)],
    buffers: &Arc<Mutex<BTreeMap<TrackId, Vec<f32>>>>,
    overflow: &Arc<AtomicBool>,
) {
    let Ok(mut buffers) = buffers.lock() else {
        overflow.store(true, Ordering::Relaxed);
        return;
    };
    for (track_id, slot) in slots {
        if bus.take_overflow(*slot) {
            overflow.store(true, Ordering::Relaxed);
        }
        bus.drain_recording_into(*slot, buffers.entry(*track_id).or_default());
    }
}
