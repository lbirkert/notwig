use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use notwig_core::dsp::CompiledPlan;
use notwig_core::protocol::{MetronomeState, RampSpec};
use notwig_core::{
    AssetId, AudioAssetBuffer, Db, DeviceId, MonitorMode, NoteId, Pan, ParamId, ParamValue,
    PlaybackSchedule, TimeMus, TrackId,
};

use crate::runtime::ActivePlan;

/// A fully prepared runtime that the audio thread can install without building
/// devices, compiling plans, loading plugins, or allocating buffers.
pub struct PreparedRuntime {
    pub(crate) plan: Arc<CompiledPlan>,
    pub(crate) schedule: Arc<PlaybackSchedule>,
    pub(crate) assets: Arc<BTreeMap<AssetId, Arc<AudioAssetBuffer>>>,
    pub(crate) runtime: ActivePlan,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TransportUpdate {
    pub playing: bool,
    pub playhead_samples: u64,
    pub loop_enabled: bool,
    pub loop_start_samples: Option<u64>,
    pub loop_end_samples: Option<u64>,
    pub stop: bool,
    pub metronome: Option<MetronomeState>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamUpdate {
    pub device_id: DeviceId,
    pub param_id: ParamId,
    pub value: ParamValue,
    pub ramp: Option<RampSpec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceBypassUpdate {
    pub device_id: DeviceId,
    pub bypass: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputRoute {
    pub track_id: TrackId,
    pub slot: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TrackStateUpdate {
    pub track_id: TrackId,
    pub mute: bool,
    pub solo_effective: bool,
    pub gain_db: Db,
    pub pan: Pan,
    pub monitor: MonitorMode,
}

/// A short UI-triggered note preview. Pending previews are staged on the app
/// thread and copied into a fixed-size pool on the audio thread.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PreviewNote {
    pub track_id: TrackId,
    pub note_id: NoteId,
    pub pitch: u8,
    pub velocity: f32,
    /// `None` holds the note until an explicit release; `Some(length)` schedules
    /// an automatic note-off.
    pub length_mus: Option<TimeMus>,
}

/// Serializable subset of [`EngineData`] sent to an out-of-process engine.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EngineControlUpdate {
    pub schedule: Option<Arc<PlaybackSchedule>>,
    pub assets: Option<Arc<BTreeMap<AssetId, Arc<AudioAssetBuffer>>>>,
    pub track_states: Vec<TrackStateUpdate>,
    pub params: Vec<ParamUpdate>,
    pub device_bypasses: Vec<DeviceBypassUpdate>,
    pub preview_notes: Vec<PreviewNote>,
    pub preview_offs: Vec<(TrackId, NoteId)>,
    pub transport: Option<TransportUpdate>,
    pub metronome: Option<MetronomeState>,
    pub input_routes: Option<Vec<InputRoute>>,
    pub low_latency_monitoring: Option<bool>,
    pub all_notes_off: bool,
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
    pub(crate) device_bypasses: Vec<DeviceBypassUpdate>,
    pub(crate) preview_notes: Vec<PreviewNote>,
    pub(crate) preview_offs: Vec<(TrackId, NoteId)>,
    pub(crate) transport: Option<TransportUpdate>,
    pub(crate) metronome: Option<MetronomeState>,
    pub(crate) input_routes: Option<Vec<InputRoute>>,
    pub(crate) low_latency_monitoring: Option<bool>,
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

    pub fn set_input_routes(&mut self, routes: Vec<InputRoute>) {
        self.input_routes = Some(routes);
    }

    pub fn set_low_latency_monitoring(&mut self, enabled: bool) {
        self.low_latency_monitoring = Some(enabled);
    }

    /// Update only the metronome without touching the transport or playhead.
    pub fn set_metronome(&mut self, metronome: MetronomeState) {
        self.metronome = Some(metronome);
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

    pub fn push_device_bypass(&mut self, update: DeviceBypassUpdate) {
        self.device_bypasses.push(update);
    }

    pub fn push_preview_note(&mut self, preview: PreviewNote) {
        self.preview_notes.push(preview);
    }

    pub fn push_preview_off(&mut self, track_id: TrackId, note_id: NoteId) {
        self.preview_offs.push((track_id, note_id));
    }

    /// Removes and returns the fields that can cross a process boundary.
    #[must_use]
    pub fn drain_control_update(&mut self) -> EngineControlUpdate {
        EngineControlUpdate {
            schedule: self.schedule.take(),
            assets: self.assets.take(),
            track_states: std::mem::take(&mut self.track_states),
            params: std::mem::take(&mut self.params),
            device_bypasses: std::mem::take(&mut self.device_bypasses),
            preview_notes: std::mem::take(&mut self.preview_notes),
            preview_offs: std::mem::take(&mut self.preview_offs),
            transport: self.transport.take(),
            metronome: self.metronome.take(),
            input_routes: self.input_routes.take(),
            low_latency_monitoring: self.low_latency_monitoring.take(),
            all_notes_off: std::mem::replace(&mut self.all_notes_off, false),
        }
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
    remote: Option<Box<dyn Fn(EngineControlUpdate) + Send>>,
}

impl EngineControl {
    #[must_use]
    pub fn new() -> Self {
        Self {
            handle: ControlHandle::new(),
            data: Box::default(),
            remote: None,
        }
    }

    #[must_use]
    pub fn handle(&self) -> ControlHandle {
        self.handle.clone()
    }

    pub fn data_mut(&mut self) -> &mut EngineData {
        &mut self.data
    }

    /// Routes [`Self::flush`] to an out-of-process engine instead of the local
    /// shared exchange.
    pub fn set_remote_sink(&mut self, sink: Option<Box<dyn Fn(EngineControlUpdate) + Send>>) {
        self.remote = sink;
    }

    /// Publish the app-side box and take back the previous box from the slot.
    /// The returned box is cleared so it can be reused for the next update.
    pub fn flush(&mut self) {
        if let Some(sink) = &self.remote {
            let update = self.data.drain_control_update();
            sink(update);
            return;
        }
        let Ok(mut exchange) = self.handle.exchange.lock() else {
            return;
        };
        let mut outgoing = std::mem::take(&mut self.data);
        // A prepared runtime is the one control value that must never be
        // discarded by a later non-prepared publish. If the audio thread has
        // not consumed the previous box yet, carry its pending runtime into the
        // outgoing box so the structural change still lands. Superseding it with
        // a new prepared runtime is fine; the app thread drops the older one.
        if let Some(previous) = exchange.data.as_mut() {
            if outgoing.prepared.is_none()
                && let Some(pending) = previous.prepared.take()
            {
                outgoing.prepared = Some(pending);
            }
            // A transport publish can be immediately followed by a control
            // publish (for example arming inputs after pressing Play). Carry a
            // transport the audio thread has not consumed yet so the later
            // publish cannot rewind or cancel it.
            if outgoing.transport.is_none()
                && let Some(pending) = previous.transport.take()
            {
                outgoing.transport = Some(pending);
            }
        }
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
