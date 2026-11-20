use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use notwig_core::protocol::{MetronomeState, RampSpec};
use notwig_core::{
    AssetId, AudioAssetBuffer, Db, DeviceId, NoteId, Pan, ParamId, ParamValue, PlaybackSchedule,
    TimeMus, TrackId,
};
use notwig_dsp::CompiledPlan;

use crate::runtime::ActivePlan;

/// A fully prepared runtime that the audio thread can install without building
/// devices, compiling plans, loading plugins, or allocating buffers.
pub struct PreparedRuntime {
    pub(crate) plan: Arc<CompiledPlan>,
    pub(crate) schedule: Arc<PlaybackSchedule>,
    pub(crate) assets: Arc<BTreeMap<AssetId, Arc<AudioAssetBuffer>>>,
    pub(crate) runtime: ActivePlan,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransportUpdate {
    pub playing: bool,
    pub playhead_samples: u64,
    pub loop_enabled: bool,
    pub loop_start_samples: Option<u64>,
    pub loop_end_samples: Option<u64>,
    pub stop: bool,
    pub metronome: Option<MetronomeState>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParamUpdate {
    pub device_id: DeviceId,
    pub param_id: ParamId,
    pub value: ParamValue,
    pub ramp: Option<RampSpec>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackStateUpdate {
    pub track_id: TrackId,
    pub mute: bool,
    pub solo_effective: bool,
    pub gain_db: Db,
    pub pan: Pan,
}

/// A short UI-triggered note preview. Pending previews are staged on the app
/// thread and copied into a fixed-size pool on the audio thread.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreviewNote {
    pub track_id: TrackId,
    pub note_id: NoteId,
    pub pitch: u8,
    pub velocity: f32,
    /// `None` holds the note until an explicit release; `Some(length)` schedules
    /// an automatic note-off.
    pub length_mus: Option<TimeMus>,
}

/// Control data produced by the app thread and consumed by the audio thread.
///
/// The audio thread swaps the whole box, applies the data, and leaves owned
/// allocations in the box so they are dropped back on the app thread.
#[derive(Default)]
pub struct EngineData {
    pub(crate) prepared: Option<Box<PreparedRuntime>>,
    pub(crate) retired_runtime: Option<ActivePlan>,
    pub(crate) retired_plan: Option<Arc<CompiledPlan>>,
    pub(crate) retired_schedule: Vec<Arc<PlaybackSchedule>>,
    pub(crate) retired_assets: Vec<Arc<BTreeMap<AssetId, Arc<AudioAssetBuffer>>>>,
    pub(crate) assets: Option<Arc<BTreeMap<AssetId, Arc<AudioAssetBuffer>>>>,
    pub(crate) schedule: Option<Arc<PlaybackSchedule>>,
    pub(crate) track_states: Vec<TrackStateUpdate>,
    pub(crate) params: Vec<ParamUpdate>,
    pub(crate) preview_notes: Vec<PreviewNote>,
    pub(crate) preview_offs: Vec<(TrackId, NoteId)>,
    pub(crate) transport: Option<TransportUpdate>,
    pub(crate) all_notes_off: bool,
}

impl EngineData {
    pub fn set_prepared_runtime(&mut self, runtime: Box<PreparedRuntime>) {
        self.prepared = Some(runtime);
    }

    pub fn set_schedule(&mut self, schedule: Arc<PlaybackSchedule>) {
        self.schedule = Some(schedule);
    }

    pub fn set_assets(&mut self, assets: Arc<BTreeMap<AssetId, Arc<AudioAssetBuffer>>>) {
        self.assets = Some(assets);
    }

    pub fn set_transport(&mut self, transport: TransportUpdate) {
        self.transport = Some(transport);
    }

    pub fn request_all_notes_off(&mut self) {
        self.all_notes_off = true;
    }

    pub fn push_track_state(&mut self, update: TrackStateUpdate) {
        self.track_states.push(update);
    }

    pub fn push_param(&mut self, update: ParamUpdate) {
        self.params.push(update);
    }

    pub fn push_preview_note(&mut self, preview: PreviewNote) {
        self.preview_notes.push(preview);
    }

    pub fn push_preview_off(&mut self, track_id: TrackId, note_id: NoteId) {
        self.preview_offs.push((track_id, note_id));
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

#[derive(Default)]
pub(crate) struct EngineExchange {
    pub(crate) data: Option<Box<EngineData>>,
    pub(crate) updated: bool,
}

impl std::fmt::Debug for EngineExchange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EngineExchange")
            .field("has_data", &self.data.is_some())
            .field("updated", &self.updated)
            .finish()
    }
}

/// Shared handle used by the audio engine to pull control data.
#[derive(Clone)]
pub struct ControlHandle {
    exchange: Arc<Mutex<EngineExchange>>,
    playhead_samples: Arc<AtomicU64>,
}

impl ControlHandle {
    #[must_use]
    pub fn new() -> Self {
        Self {
            exchange: Arc::new(Mutex::new(EngineExchange::default())),
            playhead_samples: Arc::new(AtomicU64::new(0)),
        }
    }

    #[must_use]
    pub fn playhead_samples(&self) -> u64 {
        self.playhead_samples.load(Ordering::Relaxed)
    }

    pub(crate) fn store_playhead_samples(&self, samples: u64) {
        self.playhead_samples.store(samples, Ordering::Relaxed);
    }

    pub(crate) fn exchange(&self) -> &Mutex<EngineExchange> {
        &self.exchange
    }
}

impl Default for ControlHandle {
    fn default() -> Self {
        Self::new()
    }
}

/// App-thread producer for the shared control slot.
pub struct EngineControl {
    handle: ControlHandle,
    data: Box<EngineData>,
}

impl EngineControl {
    #[must_use]
    pub fn new() -> Self {
        Self {
            handle: ControlHandle::new(),
            data: Box::default(),
        }
    }

    #[must_use]
    pub fn handle(&self) -> ControlHandle {
        self.handle.clone()
    }

    pub fn data_mut(&mut self) -> &mut EngineData {
        &mut self.data
    }

    /// Publish the app-side box and take back the previous box from the slot.
    /// The returned box is cleared so it can be reused for the next update.
    pub fn flush(&mut self) {
        let Ok(mut exchange) = self.handle.exchange.lock() else {
            return;
        };
        let outgoing = std::mem::take(&mut self.data);
        let previous = exchange.data.replace(outgoing);
        exchange.updated = true;
        drop(exchange);
        if let Some(previous) = previous {
            self.data = previous;
        }
        self.data.clear();
    }
}

impl Default for EngineControl {
    fn default() -> Self {
        Self::new()
    }
}

/// Spawns the engine-side plugin/runtime loader thread.
///
/// The loader owns device instantiation and runtime preparation. The audio
/// thread only receives the finished [`PreparedRuntime`] through the control
/// exchange.
pub struct RuntimeLoader;

impl RuntimeLoader {
    pub fn spawn<F>(
        job: F,
    ) -> Result<std::sync::mpsc::Receiver<Result<Box<PreparedRuntime>, String>>, String>
    where
        F: FnOnce() -> Result<Box<PreparedRuntime>, String> + Send + 'static,
    {
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("notwig-plugin-loader".into())
            .spawn(move || {
                let _ = sender.send(job());
            })
            .map_err(|err| format!("Could not start the plugin loader: {err}"))?;
        Ok(receiver)
    }
}
