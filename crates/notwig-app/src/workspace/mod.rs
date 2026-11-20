use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::engine_build::{EngineBuild, EngineBuildRequest};
use crate::history::History;
use crate::plugins::{PluginCatalog, new_plugin_host, plugin_entries};
use notwig_core::ids::{
    Clip as ClipMarker, Device as DeviceMarker, DeviceChain as DeviceChainMarker,
    Note as NoteMarker, Track as TrackMarker,
};
use notwig_core::protocol::{Command, CommandPayload, EngineMsg, PlaybackSchedule};
use notwig_core::sequencer::MidiClipData;
use notwig_core::{
    AssetId, AudioAssetBuffer, ClipId, ClipPayload, Db, DeviceChainId, DeviceId, DeviceKind, Id,
    Meta, NoteId, ProjectDoc, Rational, SampleRate, SchemaVersion, StateBlobRef, TrackId,
    TrackKind, apply,
};
use notwig_dsp::EngineCaps;
use notwig_engine::{
    ControlHandle, Engine, EngineControl, ParamUpdate, PreparedRuntime, PreviewNote,
    TrackStateUpdate, TransportUpdate,
};
use notwig_plugins::{PluginDescriptor, PluginState};
use notwig_ui::{Clipboard, PluginEntry};
use uuid::Uuid;

use crate::edit_lowering::lower_arranger;
use notwig_core::edit::{self, ArrangerModel, ClipContent, SelectionInfo};

mod assets;
mod recovery;

const PLUGIN_STATE_ENCODING: &str = "notwig.plugin-state.json";

pub struct Workspace {
    pub doc: ProjectDoc,
    pub bundle: Option<PathBuf>,
    pub dirty: bool,
    pub assets: BTreeMap<AssetId, Arc<AudioAssetBuffer>>,
    pub peaks: BTreeMap<AssetId, Vec<f32>>,
    pending_import_dirs: BTreeMap<AssetId, PathBuf>,
    pending_audio_imports: std::collections::BTreeSet<AssetId>,
    plugin_states: BTreeMap<DeviceId, PluginState>,
    engine_control: EngineControl,
    engine_ready: bool,
    pub plugins: Vec<PluginEntry>,
    plugin_descriptors: Vec<PluginDescriptor>,
    plugin_specs: BTreeMap<String, notwig_dsp::DeviceSpec>,
    pub status: Option<String>,
    pub error: Option<String>,
    pub last_autosave: std::time::Instant,
    pub play_start: Option<notwig_core::TimeMus>,
    pub audio_block_size: u32,
    pub recovery_offered: bool,
    clipboard: Clipboard,
    history: History,
    engine_build: EngineBuild,
}

impl Workspace {
    pub fn new_empty() -> Result<Self, String> {
        let meta = Meta {
            name: "Untitled".into(),
            created_utc: 0,
            modified_utc: 0,
            sample_rate: SampleRate::DEFAULT,
            tempo_bpm: 120.0,
            time_sig_num: 4,
            time_sig_den: 4,
        };
        let doc = ProjectDoc::new_template(
            meta,
            Id::from_uuid(Uuid::new_v4()),
            Id::from_uuid(Uuid::new_v4()),
        )
        .map_err(|err| err.to_string())?;
        Ok(Self::from_doc(doc, None, true))
    }

    pub fn open(bundle: &Path) -> Result<Self, String> {
        let loaded = notwig_project::open(bundle).map_err(|err| err.to_string())?;
        let mut workspace = Self::from_doc(loaded.doc, Some(bundle.to_path_buf()), false);
        if !loaded.issues.is_empty() {
            let summary = loaded
                .issues
                .iter()
                .map(|issue| format!("{}: {}", issue.entity, issue.message))
                .collect::<Vec<_>>()
                .join("; ");
            workspace.status = Some(format!("Project loaded with issues: {summary}"));
        }
        workspace.recovery_offered = loaded.recovery_offered;
        if loaded.recovery_offered {
            workspace.status = Some("Recovery autosaves are available.".into());
        }
        workspace.begin_session();
        Ok(workspace)
    }

    fn from_doc(doc: ProjectDoc, bundle: Option<PathBuf>, dirty: bool) -> Self {
        let plugin_descriptors = PluginCatalog::global().descriptors();
        let plugins = plugin_entries(&plugin_descriptors);
        let engine_control = EngineControl::new();
        let mut workspace = Self {
            doc,
            bundle,
            dirty,
            assets: BTreeMap::new(),
            peaks: BTreeMap::new(),
            pending_import_dirs: BTreeMap::new(),
            pending_audio_imports: std::collections::BTreeSet::new(),
            plugin_states: BTreeMap::new(),
            engine_control,
            engine_ready: false,
            plugins,
            plugin_descriptors,
            plugin_specs: BTreeMap::new(),
            status: None,
            error: None,
            last_autosave: std::time::Instant::now(),
            play_start: None,
            audio_block_size: 512,
            recovery_offered: false,
            clipboard: Clipboard::default(),
            history: History::default(),
            engine_build: EngineBuild::new(),
        };
        workspace.refresh_plugin_specs();
        workspace.load_assets();
        workspace.load_plugin_states();
        workspace.request_engine_rebuild();
        workspace
    }

    #[allow(dead_code)]
    pub fn rebuild_engine(&mut self) -> Result<(), String> {
        let prepared = self.engine_build_request().prepare()?;
        self.install_prepared_runtime(prepared);
        Ok(())
    }

    fn preserved_playhead_samples(&self) -> u64 {
        let engine_playhead = self.engine_control.handle().playhead_samples();
        if engine_playhead > 0 {
            engine_playhead
        } else {
            self.doc
                .tempo_map()
                .mus_to_samples(self.doc.transport.playhead)
        }
    }

    #[must_use]
    pub fn engine_control_handle(&self) -> ControlHandle {
        self.engine_control.handle()
    }

    #[must_use]
    pub fn engine_ready(&self) -> bool {
        self.engine_ready
    }

    #[doc(hidden)]
    pub fn offline_engine(&self) -> Result<Engine, String> {
        self.build_engine()
    }

    fn build_engine(&self) -> Result<Engine, String> {
        self.engine_build_request().build()
    }

    fn engine_build_request(&self) -> EngineBuildRequest {
        let sample_rate = self.doc.meta.sample_rate.value();
        let max_block_size = self.audio_block_size.max(16) as usize;
        EngineBuildRequest {
            doc: self.doc.clone(),
            caps: EngineCaps {
                sample_rate,
                max_block_size,
                channels: 2,
            },
            registry: self.device_registry(),
            plugin_descriptors: self.plugin_descriptors.clone(),
            plugin_states: self.plugin_states.clone(),
            assets: self.assets.clone(),
            sample_rate,
            max_block_size,
        }
    }

    pub fn request_engine_rebuild(&mut self) {
        self.status = Some("Loading instruments and effects…".into());
        let request = self.engine_build_request();
        if let Err(err) = self.engine_build.request(request) {
            self.error = Some(err.clone());
            self.status = Some(err);
        }
    }

    pub fn engine_building(&self) -> bool {
        self.engine_build.building()
    }

    #[allow(dead_code)]
    pub fn engine_build_pending(&self) -> bool {
        self.engine_build.building()
    }

    #[allow(dead_code)]
    #[must_use]
    pub fn engine_build_requests(&self) -> u64 {
        self.engine_build.requests()
    }

    pub fn poll_engine_build(&mut self) -> bool {
        let Some(outcome) = self.engine_build.poll() else {
            return false;
        };
        match outcome {
            Ok(runtime) => {
                self.install_prepared_runtime(runtime);
                self.status = Some("Instruments and effects loaded.".into());
            }
            Err(err) => {
                self.error = Some(err.clone());
                self.status = Some(format!("Failed to load instruments and effects: {err}"));
            }
        }
        true
    }

    fn install_prepared_runtime(&mut self, runtime: Box<PreparedRuntime>) {
        let transport =
            (!self.engine_ready).then(|| self.transport_update(self.preserved_playhead_samples()));
        self.engine_ready = true;
        self.publish_engine_control(transport, false, Some(runtime));
    }

    fn sync_engine(&mut self) {
        self.publish_engine_control(None, false, None);
    }

    fn transport_update(&self, playhead_samples: u64) -> TransportUpdate {
        let map = self.doc.tempo_map();
        let (loop_start, loop_end) = self.doc.transport.loop_range.map_or((None, None), |range| {
            (
                Some(map.mus_to_samples(range.start)),
                Some(map.mus_to_samples(range.end)),
            )
        });
        TransportUpdate {
            playing: self.doc.transport.playing,
            playhead_samples,
            loop_enabled: self.doc.transport.loop_enabled,
            loop_start_samples: loop_start,
            loop_end_samples: loop_end,
            stop: false,
            metronome: Some(notwig_core::protocol::MetronomeState {
                enabled: self.doc.transport.metronome_enabled,
                beats_per_bar: self.doc.meta.time_sig_num,
            }),
        }
    }

    fn publish_engine_control(
        &mut self,
        transport: Option<TransportUpdate>,
        all_notes_off: bool,
        prepared: Option<Box<PreparedRuntime>>,
    ) {
        let schedule = Arc::new(PlaybackSchedule::from_doc(&self.doc));
        let data = self.engine_control.data_mut();
        if let Some(prepared) = prepared {
            data.set_prepared_runtime(prepared);
        }
        data.set_schedule(schedule);
        for track in self.doc.tracks.values() {
            data.push_track_state(TrackStateUpdate {
                track_id: track.id,
                mute: track.mixer.mute,
                solo_effective: self.doc.effective_solo(track.id),
                gain_db: track.mixer.volume_db,
                pan: track.mixer.pan,
            });
        }
        for device in self.doc.devices.values() {
            for (param_id, value) in &device.param_values {
                data.push_param(ParamUpdate {
                    device_id: device.id,
                    param_id: *param_id,
                    value: value.clone(),
                    ramp: None,
                });
            }
        }
        if let Some(transport) = transport {
            data.set_transport(transport);
        }
        if all_notes_off {
            data.request_all_notes_off();
        }
        self.engine_control.flush();
    }

    /// Re-publish transport and force a fresh prepared runtime after the audio
    /// stream is recreated.
    pub fn engine_audio_restarted(&mut self) {
        self.engine_ready = false;
        let transport = self.transport_update(self.preserved_playhead_samples());
        self.publish_engine_control(Some(transport), false, None);
        self.request_engine_rebuild();
    }

    pub fn apply(&mut self, payload: CommandPayload) -> Result<(), String> {
        let preserve_playhead = matches!(
            &payload,
            CommandPayload::SetTransport {
                playing: None,
                playhead: None,
                ..
            }
        );
        let structural = payload.requires_plan_rebuild();
        let record_undo = !matches!(
            payload,
            CommandPayload::SetTransport { .. } | CommandPayload::Pause | CommandPayload::Stop
        );
        let before = record_undo.then(|| self.doc.clone());
        match apply(&self.doc, &Command::new(payload)) {
            Ok(result) => {
                self.doc = result.doc;
                self.dirty = true;
                if let Some(before) = before {
                    self.history.record(before, structural);
                }
                if structural {
                    self.request_engine_rebuild();
                } else {
                    let (mut transport, all_notes_off) = control_updates(result.engine_msgs);
                    if preserve_playhead && let Some(update) = transport.as_mut() {
                        update.playhead_samples = self.preserved_playhead_samples();
                    }
                    self.publish_engine_control(transport, all_notes_off, None);
                }
                self.error = None;
                Ok(())
            }
            Err(err) => {
                self.error = Some(err.to_string());
                Err(err.to_string())
            }
        }
    }

    pub fn undo(&mut self) -> Result<(), String> {
        let Some(entry) = self.history.undo(&self.doc) else {
            return Ok(());
        };
        self.doc = entry.doc;
        self.dirty = true;
        if entry.structural {
            self.request_engine_rebuild();
        } else {
            self.sync_engine();
        }
        self.status = Some("Undo.".into());
        Ok(())
    }

    pub fn redo(&mut self) -> Result<(), String> {
        let Some(entry) = self.history.redo(&self.doc) else {
            return Ok(());
        };
        self.doc = entry.doc;
        self.dirty = true;
        if entry.structural {
            self.request_engine_rebuild();
        } else {
            self.sync_engine();
        }
        self.status = Some("Redo.".into());
        Ok(())
    }

    pub fn save(&mut self, as_path: Option<&Path>) -> Result<PathBuf, String> {
        let path = match as_path {
            Some(path) => path.to_path_buf(),
            None => self
                .bundle
                .clone()
                .ok_or_else(|| "project has no bundle path; use Save As".to_string())?,
        };
        if !self.pending_audio_imports.is_empty() {
            return Err(
                "Audio import is still running; wait for it to finish before saving.".into(),
            );
        }
        let previous = self.bundle.clone();
        if !self.plugin_states.is_empty() {
            let state_dir = path.join("state");
            std::fs::create_dir_all(&state_dir).map_err(|err| err.to_string())?;
            for (device_id, state) in &self.plugin_states {
                let bytes = serde_json::to_vec(state).map_err(|err| err.to_string())?;
                std::fs::write(state_dir.join(format!("{device_id}.json")), bytes)
                    .map_err(|err| err.to_string())?;
                if let Some(device) = self.doc.devices.get_mut(device_id) {
                    device.state_ref = Some(StateBlobRef {
                        hash: device_id.to_string(),
                        encoding: PLUGIN_STATE_ENCODING.into(),
                    });
                }
            }
        }
        if !self.pending_import_dirs.is_empty() {
            let assets_dir = path.join("assets");
            std::fs::create_dir_all(&assets_dir).map_err(|err| err.to_string())?;
            for (asset_id, dir) in &self.pending_import_dirs {
                let Some(entry) = self.doc.assets.assets.get(asset_id) else {
                    continue;
                };
                let file_name = Path::new(&entry.uri).file_name().map_or_else(
                    || entry.uri.clone(),
                    |name| name.to_string_lossy().into_owned(),
                );
                std::fs::copy(dir.join(&entry.uri), assets_dir.join(file_name))
                    .map_err(|err| err.to_string())?;
            }
        }
        notwig_project::save(&self.doc, &path).map_err(|err| err.to_string())?;
        for dir in self.pending_import_dirs.values() {
            let _ = std::fs::remove_dir_all(dir);
        }
        self.pending_import_dirs.clear();
        self.bundle = Some(path.clone());
        if previous.as_deref() != Some(path.as_path())
            && let Some(previous) = previous
            && let Err(err) = notwig_project::remove_session_lock(&previous)
        {
            tracing::warn!(
                path = %previous.display(),
                error = %err,
                "could not remove old session lock"
            );
        }
        self.dirty = false;
        self.begin_session();
        self.status = Some(format!("Saved {}", path.display()));
        Ok(path)
    }

    pub fn autosave_if_due(&mut self) {
        if !self.dirty || self.last_autosave.elapsed().as_secs() < 60 {
            return;
        }
        let Some(bundle) = self.bundle.clone() else {
            return;
        };
        if !self.pending_audio_imports.is_empty() {
            return;
        }
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| {
                i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
            });
        let _ = notwig_project::autosave(&self.doc, &bundle, timestamp);
        self.last_autosave = std::time::Instant::now();
    }

    fn load_plugin_states(&mut self) {
        self.plugin_states.clear();
        let Some(bundle) = self.bundle.clone() else {
            return;
        };
        for device in self.doc.devices.values() {
            let Some(state_ref) = &device.state_ref else {
                continue;
            };
            if state_ref.encoding != PLUGIN_STATE_ENCODING {
                continue;
            }
            let path = bundle
                .join("state")
                .join(format!("{}.json", state_ref.hash));
            let Ok(bytes) = std::fs::read(path) else {
                continue;
            };
            if let Ok(state) = serde_json::from_slice::<PluginState>(&bytes) {
                self.plugin_states.insert(device.id, state);
            }
        }
    }

    #[must_use]
    pub fn plugin_state(&self, device_id: DeviceId) -> Option<&PluginState> {
        self.plugin_states.get(&device_id)
    }

    pub fn store_plugin_state(&mut self, device_id: DeviceId, state: PluginState) {
        self.plugin_states.insert(device_id, state);
        if let Some(device) = self.doc.devices.get_mut(&device_id) {
            device.state_ref = Some(StateBlobRef {
                hash: device_id.to_string(),
                encoding: PLUGIN_STATE_ENCODING.into(),
            });
        }
        self.dirty = true;
    }

    pub fn set_plugin_state(&mut self, device_id: DeviceId, state: PluginState) {
        self.store_plugin_state(device_id, state);
        self.request_engine_rebuild();
    }

    pub fn set_sample_rate(&mut self, sample_rate: u32) -> Result<(), String> {
        let rate = notwig_core::SampleRate::new(sample_rate).map_err(|err| err.to_string())?;
        let previous = self.doc.meta.sample_rate;
        if previous == rate {
            return Ok(());
        }
        let ratio = f64::from(rate.value()) / f64::from(previous.value().max(1));

        let loaded: Vec<AssetId> = self.assets.keys().copied().collect();
        for asset_id in loaded {
            let Some(buffer) = self.assets.get(&asset_id).cloned() else {
                continue;
            };
            let converted = notwig_project::resample(
                &buffer,
                rate.value(),
                notwig_project::ResampleQuality::High,
            )
            .map_err(|err| err.to_string())?;
            self.peaks
                .insert(asset_id, assets::compute_peaks(&converted, 2048));
            self.persist_asset_file(asset_id, &converted)?;
            if let Some(entry) = self.doc.assets.assets.get_mut(&asset_id) {
                entry.sample_rate = Some(rate.value());
                entry.duration_samples = Some(converted.frame_count());
            }
            self.assets.insert(asset_id, Arc::new(converted));
        }

        for clip in self.doc.arranger.clips.values_mut() {
            let notwig_core::ClipPayload::Audio(audio) = &mut clip.payload else {
                continue;
            };
            if audio.uses_slices() {
                for slice in &mut audio.slices {
                    slice.source_start_sample = scale_sample(slice.source_start_sample, ratio);
                    slice.source_end_sample = scale_sample(slice.source_end_sample, ratio);
                    slice.silent_prefix_samples = scale_sample(slice.silent_prefix_samples, ratio);
                }
            } else {
                audio.source_start_sample = scale_sample(audio.source_start_sample, ratio);
                audio.source_end_sample = scale_sample(audio.source_end_sample, ratio);
                audio.fade_in_samples = scale_sample(audio.fade_in_samples, ratio);
                audio.fade_out_samples = scale_sample(audio.fade_out_samples, ratio);
            }
        }

        self.doc.meta.sample_rate = rate;
        self.dirty = true;
        self.prepare_asset();
        self.request_engine_rebuild();
        Ok(())
    }

    pub fn set_playhead(&mut self, playhead: notwig_core::TimeMus) -> Result<(), String> {
        self.apply(CommandPayload::SetTransport {
            playing: None,
            playhead: Some(playhead),
            loop_enabled: None,
            loop_range: None,
            metronome: None,
        })
    }

    /// Stage a short, UI-triggered note preview for the audio engine.
    pub fn preview_note(
        &mut self,
        track_id: TrackId,
        note_id: NoteId,
        pitch: u8,
        velocity: f32,
        length_mus: Option<notwig_core::TimeMus>,
    ) {
        self.engine_control
            .data_mut()
            .push_preview_note(PreviewNote {
                track_id,
                note_id,
                pitch,
                velocity,
                length_mus,
            });
        self.engine_control.flush();
    }

    pub fn release_preview_note(&mut self, track_id: TrackId, note_id: NoteId) {
        self.engine_control
            .data_mut()
            .push_preview_off(track_id, note_id);
        self.engine_control.flush();
    }

    pub fn play(&mut self) -> Result<(), String> {
        if !self.doc.transport.playing {
            self.play_start = Some(self.doc.transport.playhead);
        }
        self.apply(CommandPayload::SetTransport {
            playing: Some(true),
            playhead: Some(self.doc.transport.playhead),
            loop_enabled: None,
            loop_range: None,
            metronome: None,
        })
    }

    pub fn pause(&mut self) -> Result<(), String> {
        self.apply(CommandPayload::Pause)
    }

    pub fn stop(&mut self) -> Result<(), String> {
        let start = self
            .play_start
            .take()
            .unwrap_or(self.doc.transport.playhead);
        self.apply(CommandPayload::SetTransport {
            playing: Some(false),
            playhead: Some(start),
            loop_enabled: None,
            loop_range: None,
            metronome: None,
        })
    }

    pub fn toggle_loop(&mut self) -> Result<(), String> {
        let enabled = !self.doc.transport.loop_enabled;
        let range = if enabled && self.doc.transport.loop_range.is_none() {
            Some(
                notwig_core::RangeMus::new(notwig_core::TimeMus::zero(), Rational::new(4, 1))
                    .map_err(|err| err.to_string())?,
            )
        } else {
            self.doc.transport.loop_range
        };
        self.apply(CommandPayload::SetTransport {
            playing: None,
            playhead: None,
            loop_enabled: Some(enabled),
            loop_range: Some(range),
            metronome: None,
        })
    }

    pub fn create_track(&mut self, kind: TrackKind) -> Result<(), String> {
        self.create_track_at(kind, None).map(|_| ())
    }

    pub fn create_track_at(
        &mut self,
        kind: TrackKind,
        index: Option<usize>,
    ) -> Result<TrackId, String> {
        self.create_track_with_parent(kind, None, index)
    }

    fn create_track_with_parent(
        &mut self,
        kind: TrackKind,
        parent: Option<TrackId>,
        index: Option<usize>,
    ) -> Result<TrackId, String> {
        let track = Id::<TrackMarker>::from_uuid(Uuid::new_v4());
        let chain = Id::<DeviceChainMarker>::from_uuid(Uuid::new_v4());
        let name = match kind {
            TrackKind::Group => format!("Group {}", self.doc.tracks.len()),
            _ => format!("{} {}", kind.name(), self.doc.tracks.len()),
        };
        self.apply(CommandPayload::CreateTrack {
            id: track,
            chain_id: chain,
            kind,
            name,
            parent,
            index,
            color: None,
        })?;
        Ok(track)
    }

    pub fn create_midi_clip(
        &mut self,
        track_id: TrackId,
        range: notwig_core::RangeMus,
    ) -> Result<(), String> {
        let id = Id::<ClipMarker>::from_uuid(Uuid::new_v4());
        self.apply(CommandPayload::CreateClip {
            id,
            track_id,
            kind: notwig_core::ClipKind::Midi,
            range,
            payload: ClipPayload::Midi(MidiClipData {
                length: range.length(),
                content_start: notwig_core::TimeMus::zero(),
                notes: Vec::new(),
            }),
        })
    }

    pub fn create_note(
        &mut self,
        clip_id: ClipId,
        note: notwig_core::NoteEntity,
    ) -> Result<(), String> {
        self.apply(CommandPayload::CreateNote { clip_id, note })
    }

    pub fn edit_notes(
        &mut self,
        clip_id: ClipId,
        notes: Vec<notwig_core::NoteEntity>,
        removed: Vec<notwig_core::NoteId>,
    ) -> Result<(), String> {
        let mut payloads = Vec::new();
        if !removed.is_empty() {
            payloads.push(CommandPayload::DeleteNotes {
                clip_id,
                note_ids: removed,
            });
        }
        if !notes.is_empty() {
            payloads.push(CommandPayload::SetNotes { clip_id, notes });
        }
        self.apply_batch(payloads)
    }

    pub fn create_audio_clip(
        &mut self,
        track_id: TrackId,
        range: notwig_core::RangeMus,
    ) -> Result<(), String> {
        let Some(asset_id) = self
            .doc
            .assets
            .assets
            .values()
            .find(|asset| asset.kind == notwig_core::AssetKind::Audio)
            .map(|asset| asset.id)
        else {
            return Err("Import audio before creating an audio clip.".into());
        };
        let id = Id::<ClipMarker>::from_uuid(Uuid::new_v4());
        self.apply(CommandPayload::CreateClip {
            id,
            track_id,
            kind: notwig_core::ClipKind::Audio,
            range,
            payload: ClipPayload::Audio(notwig_core::AudioClipData {
                asset_id,
                source_start_sample: 0,
                source_end_sample: 1,
                gain_db: Db(0.0),
                fade_in_samples: 0,
                fade_out_samples: 0,
                fade_shape: notwig_core::sequencer::FadeShape::Linear,
                slices: Vec::new(),
                slices_enabled: true,
            }),
        })
    }

    pub fn split_clip(&mut self, clip_id: ClipId, at: notwig_core::TimeMus) -> Result<(), String> {
        let Some(clip) = self.doc.arranger.clips.get(&clip_id).cloned() else {
            return Err("clip not found".into());
        };
        if at <= clip.range.start || at >= clip.range.end {
            return Err("split position must lie inside the clip".into());
        }
        let new_clip = Id::<ClipMarker>::from_uuid(Uuid::new_v4());
        let mut new_note_ids = Vec::new();
        if let ClipPayload::Midi(midi) = &clip.payload {
            let split_offset = at - clip.range.start;
            let split_content = midi.content_start + split_offset;
            for note in &midi.notes {
                if note.start < split_content && note.start + note.length > split_content {
                    new_note_ids.push(Id::<NoteMarker>::from_uuid(Uuid::new_v4()));
                }
            }
        }
        self.apply(CommandPayload::SplitClip {
            clip_id,
            at,
            new_clip_id: new_clip,
            new_note_ids,
        })
    }

    pub fn duplicate_clip(&mut self, clip_id: ClipId) -> Result<ClipId, String> {
        let selection = self.duplicate_selection(&[clip_id], None)?;
        selection
            .objects
            .first()
            .map(|entry| entry.object)
            .ok_or_else(|| "nothing duplicated".to_string())
    }

    pub fn join_clips(
        &mut self,
        clip_ids: &[ClipId],
    ) -> Result<SelectionInfo<TrackId, ClipId>, String> {
        let plan = {
            let model = ArrangerModel::new(&self.doc);
            edit::join_clips(&model, clip_ids).map_err(|err| err.to_string())?
        };
        self.lower_and_apply(plan).map(Option::unwrap_or_default)
    }

    pub fn group_tracks(&mut self, tracks: &[TrackId]) -> Result<TrackId, String> {
        let selected: Vec<TrackId> = self
            .doc
            .track_order
            .depth_first()
            .into_iter()
            .filter(|id| tracks.contains(id))
            .filter(|id| {
                self.doc
                    .tracks
                    .get(id)
                    .is_some_and(|track| track.kind != TrackKind::Master)
            })
            .collect();
        if selected.is_empty() {
            return Err("select at least one track to group".into());
        }
        let first = selected[0];
        let first_track = self
            .doc
            .tracks
            .get(&first)
            .ok_or_else(|| "track not found".to_string())?;
        let parent = first_track.parent;
        let siblings = match parent {
            Some(parent) => self
                .doc
                .track_order
                .children
                .get(&parent)
                .cloned()
                .unwrap_or_default(),
            None => self.doc.track_order.roots.clone(),
        };
        let index = siblings
            .iter()
            .position(|id| *id == first)
            .unwrap_or(siblings.len());
        let group = self.create_track_with_parent(TrackKind::Group, parent, Some(index))?;
        for track in selected {
            let next = self
                .doc
                .track_order
                .children
                .get(&group)
                .map_or(0, Vec::len);
            self.apply(CommandPayload::MoveTrack {
                id: track,
                parent: Some(group),
                index: next,
            })?;
        }
        Ok(group)
    }

    pub fn duplicate_track(&mut self, track_id: TrackId) -> Result<TrackId, String> {
        let track = self
            .doc
            .tracks
            .get(&track_id)
            .cloned()
            .ok_or_else(|| "track not found".to_string())?;
        if track.kind == TrackKind::Master {
            return Err("Master track cannot be duplicated".into());
        }
        let parent = track.parent;
        let siblings = match parent {
            Some(parent) => self
                .doc
                .track_order
                .children
                .get(&parent)
                .cloned()
                .unwrap_or_default(),
            None => self.doc.track_order.roots.clone(),
        };
        let index = siblings
            .iter()
            .position(|id| *id == track_id)
            .map_or(siblings.len(), |index| index + 1);
        let new_track = self.create_track_with_parent(track.kind, parent, Some(index))?;
        self.apply(CommandPayload::RenameTrack {
            id: new_track,
            name: format!("{} copy", track.name),
        })?;
        self.apply(CommandPayload::SetTrackMixer {
            id: new_track,
            volume_db: Some(track.mixer.volume_db),
            pan: Some(track.mixer.pan),
            mute: Some(track.mixer.mute),
            solo: Some(track.mixer.solo),
        })?;
        if !track.active {
            self.apply(CommandPayload::SetTrackActive {
                id: new_track,
                active: false,
            })?;
        }
        if track.io.audio_out != notwig_core::tracks::TrackOutput::Default {
            self.apply(CommandPayload::SetTrackRoute {
                id: new_track,
                audio_out: track.io.audio_out,
            })?;
        }
        let new_chain = self
            .doc
            .tracks
            .get(&new_track)
            .ok_or_else(|| "duplicated track missing".to_string())?
            .main_chain;
        self.duplicate_chain_devices(track.main_chain, new_chain)?;

        let clips: Vec<notwig_core::ClipEntity> = self
            .doc
            .arranger
            .lanes
            .get(&track_id)
            .map(|lane| lane.clips.clone())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|clip_id| self.doc.arranger.clips.get(&clip_id).cloned())
            .collect();
        for clip in clips {
            let mut payload = clip.payload.clone();
            if let ClipPayload::Midi(midi) = &mut payload {
                for note in &mut midi.notes {
                    note.id = Id::<NoteMarker>::from_uuid(Uuid::new_v4());
                }
            }
            self.apply(CommandPayload::CreateClip {
                id: Id::<ClipMarker>::from_uuid(Uuid::new_v4()),
                track_id: new_track,
                kind: clip.kind,
                range: clip.range,
                payload,
            })?;
        }
        Ok(new_track)
    }

    fn duplicate_chain_devices(
        &mut self,
        from_chain: DeviceChainId,
        to_chain: DeviceChainId,
    ) -> Result<(), String> {
        let devices = self
            .doc
            .chains
            .get(&from_chain)
            .map(|chain| chain.devices.clone())
            .unwrap_or_default();
        for (index, device_id) in devices.into_iter().enumerate() {
            let Some(device) = self.doc.devices.get(&device_id).cloned() else {
                continue;
            };
            let new_device = Id::<DeviceMarker>::from_uuid(Uuid::new_v4());
            let child_chains: Vec<DeviceChainId> = device
                .child_chains
                .iter()
                .map(|_| Id::<DeviceChainMarker>::from_uuid(Uuid::new_v4()))
                .collect();
            self.apply(CommandPayload::InsertDevice {
                id: new_device,
                type_id: device.type_id.clone(),
                version: Some(device.version),
                kind: device.kind,
                target_chain: to_chain,
                index: Some(index),
                child_chain_ids: child_chains.clone(),
                name: device.name.clone(),
            })?;
            for (old_child, new_child) in device.child_chains.iter().zip(&child_chains) {
                self.duplicate_chain_devices(*old_child, *new_child)?;
            }
            if device.bypass {
                self.apply(CommandPayload::SetDeviceBypass {
                    device_id: new_device,
                    bypass: true,
                })?;
            }
            self.apply(CommandPayload::SetDeviceState {
                device_id: new_device,
                state_ref: device.state_ref.clone(),
            })?;
            for (param_id, value) in device.param_values {
                self.apply(CommandPayload::SetParam {
                    device_id: new_device,
                    param_id,
                    value,
                    ramp: None,
                })?;
            }
        }
        Ok(())
    }

    pub fn insert_stock(
        &mut self,
        chain_id: DeviceChainId,
        type_id: &str,
        kind: DeviceKind,
        index: Option<usize>,
    ) -> Result<(), String> {
        let id = Id::<DeviceMarker>::from_uuid(Uuid::new_v4());
        let child_chains = if kind == DeviceKind::Container {
            vec![Id::<DeviceChainMarker>::from_uuid(Uuid::new_v4())]
        } else {
            Vec::new()
        };
        self.apply(CommandPayload::InsertDevice {
            id,
            type_id: type_id.into(),
            version: Some(SchemaVersion::V0),
            kind,
            target_chain: chain_id,
            index,
            child_chain_ids: child_chains,
            name: None,
        })
    }

    pub fn plugin_descriptor(&self, device_id: DeviceId) -> Option<PluginDescriptor> {
        let type_id = &self.doc.devices.get(&device_id)?.type_id;
        self.plugin_descriptors
            .iter()
            .find(|descriptor| descriptor.type_id == *type_id)
            .cloned()
    }

    pub fn poll_plugin_catalog(&mut self) -> bool {
        if !PluginCatalog::global().poll() {
            return false;
        }
        self.plugin_descriptors = PluginCatalog::global().descriptors();
        self.plugins = plugin_entries(&self.plugin_descriptors);
        self.refresh_plugin_specs();
        if self
            .doc
            .devices
            .values()
            .any(|device| device.type_id.starts_with("vst3.") || device.type_id.starts_with("au."))
        {
            self.request_engine_rebuild();
        }
        true
    }

    pub fn plugin_specs(&self) -> impl Iterator<Item = &notwig_dsp::DeviceSpec> {
        self.plugin_specs.values()
    }

    fn device_registry(&self) -> notwig_dsp::DeviceRegistry {
        let mut registry = notwig_dsp::stock_registry();
        for spec in self.plugin_specs.values() {
            registry.register(spec.clone());
        }
        registry
    }

    fn refresh_plugin_specs(&mut self) {
        let type_ids: Vec<String> = self
            .doc
            .devices
            .values()
            .filter(|device| {
                device.type_id.starts_with("vst3.") || device.type_id.starts_with("au.")
            })
            .map(|device| device.type_id.clone())
            .collect();
        for type_id in type_ids {
            self.ensure_plugin_spec(&type_id);
        }
    }

    fn ensure_plugin_spec(&mut self, type_id: &str) {
        if self.plugin_specs.contains_key(type_id) {
            return;
        }
        let Some(descriptor) = self
            .plugin_descriptors
            .iter()
            .find(|descriptor| descriptor.type_id == type_id)
            .cloned()
        else {
            return;
        };
        let mut host = new_plugin_host();
        let Ok(instance) = host.load(&descriptor) else {
            return;
        };
        let params = instance.describe_parameters();
        let capabilities = instance.capabilities();
        tracing::info!(
            plugin = %descriptor.name,
            params = params.len(),
            latency = capabilities.latency_samples,
            "loaded plugin parameter metadata"
        );
        self.plugin_specs.insert(
            type_id.to_owned(),
            notwig_dsp::DeviceSpec {
                type_id: type_id.to_owned(),
                version: notwig_core::SchemaVersion::V0,
                kind: if descriptor.is_instrument {
                    DeviceKind::Instrument
                } else {
                    DeviceKind::AudioFx
                },
                ports: Vec::new(),
                params,
                latency_samples: capabilities.latency_samples,
                name: descriptor.name,
                vendor: descriptor.vendor,
            },
        );
    }

    pub fn insert_plugin(
        &mut self,
        chain_id: DeviceChainId,
        type_id: &str,
        kind: DeviceKind,
        index: Option<usize>,
    ) -> Result<DeviceId, String> {
        let id = Id::<DeviceMarker>::from_uuid(Uuid::new_v4());
        self.ensure_plugin_spec(type_id);
        let name = self
            .plugin_descriptors
            .iter()
            .find(|descriptor| descriptor.type_id == type_id)
            .map(|descriptor| descriptor.name.clone());
        self.apply(CommandPayload::InsertDevice {
            id,
            type_id: type_id.into(),
            version: None,
            kind,
            target_chain: chain_id,
            index,
            child_chain_ids: Vec::new(),
            name,
        })?;
        Ok(id)
    }

    pub fn set_mixer(
        &mut self,
        track_id: TrackId,
        volume_db: Option<Db>,
        pan: Option<notwig_core::Pan>,
        mute: Option<bool>,
        solo: Option<bool>,
    ) -> Result<(), String> {
        self.apply(CommandPayload::SetTrackMixer {
            id: track_id,
            volume_db,
            pan,
            mute,
            solo,
        })
    }

    pub fn insert_audio_clip(
        &mut self,
        track_id: TrackId,
        asset_id: AssetId,
    ) -> Result<(), String> {
        self.insert_audio_clip_at(track_id, asset_id, self.doc.transport.playhead)
    }

    pub fn insert_audio_clip_at(
        &mut self,
        track_id: TrackId,
        asset_id: AssetId,
        start: notwig_core::TimeMus,
    ) -> Result<(), String> {
        let entry = self
            .doc
            .assets
            .assets
            .get(&asset_id)
            .ok_or_else(|| "asset not found".to_string())?
            .clone();
        let duration = entry
            .duration_samples
            .ok_or_else(|| "asset has no duration".to_string())?;
        let length = self.doc.tempo_map().samples_to_mus(duration);
        let range =
            notwig_core::RangeMus::new(start, start + length).map_err(|err| err.to_string())?;
        let id = Id::<ClipMarker>::from_uuid(Uuid::new_v4());
        let mut payloads = vec![CommandPayload::CreateClip {
            id,
            track_id,
            kind: notwig_core::ClipKind::Audio,
            range,
            payload: ClipPayload::Audio(notwig_core::AudioClipData {
                asset_id,
                source_start_sample: 0,
                source_end_sample: duration,
                gain_db: Db(0.0),
                fade_in_samples: 0,
                fade_out_samples: 0,
                fade_shape: notwig_core::FadeShape::Linear,
                slices: Vec::new(),
                slices_enabled: false,
            }),
        }];
        if let Some(name) = entry.original_name {
            payloads.push(CommandPayload::EditClip {
                clip_id: id,
                range: None,
                name: Some(Some(name)),
                muted: None,
                content_start: None,
                content_length: None,
                audio: None,
            });
        }
        self.apply_batch(payloads)
    }

    pub fn relocate_clips(
        &mut self,
        clips: &[(ClipId, TrackId, notwig_core::RangeMus)],
    ) -> Result<(), String> {
        let mut ordered = clips.to_vec();
        let moving_right = ordered
            .first()
            .and_then(|(id, _, range)| {
                self.doc
                    .arranger
                    .clips
                    .get(id)
                    .map(|clip| range.start > clip.range.start)
            })
            .unwrap_or(false);
        ordered.sort_by(|a, b| {
            if moving_right {
                b.2.start.cmp(&a.2.start)
            } else {
                a.2.start.cmp(&b.2.start)
            }
        });
        let moves: Vec<edit::ObjectMove<TrackId, ClipId>> = ordered
            .iter()
            .map(|(object, lane, span)| edit::ObjectMove {
                object: *object,
                lane: *lane,
                span: *span,
            })
            .collect();
        let plan = {
            let model = ArrangerModel::new(&self.doc);
            edit::relocate_objects(&model, &moves).map_err(|err| err.to_string())?
        };
        self.lower_and_apply(plan).map(|_| ())
    }

    pub fn set_clip_range(
        &mut self,
        clip_id: ClipId,
        range: notwig_core::RangeMus,
    ) -> Result<(), String> {
        self.apply(CommandPayload::SetClipRange { clip_id, range })
    }

    pub fn trim_clip(
        &mut self,
        clip_id: ClipId,
        range: notwig_core::RangeMus,
    ) -> Result<(), String> {
        let Some(clip) = self.doc.arranger.clips.get(&clip_id).cloned() else {
            return Err("clip not found".into());
        };
        let edge = if range.start != clip.range.start && range.end == clip.range.end {
            Some((edit::Edge::Start, range.start))
        } else if range.end != clip.range.end && range.start == clip.range.start {
            Some((edit::Edge::End, range.end))
        } else {
            None
        };
        let plan = {
            let model = ArrangerModel::new(&self.doc);
            match edge {
                Some((edge, to)) => Some(
                    edit::trim_object(&model, clip_id, edge, to).map_err(|err| err.to_string())?,
                ),
                None => None,
            }
        };
        if let Some(plan) = plan {
            self.lower_and_apply(plan).map(|_| ())
        } else {
            self.apply(CommandPayload::EditClip {
                clip_id,
                range: Some(range),
                name: None,
                muted: None,
                content_start: None,
                content_length: None,
                audio: None,
            })
        }
    }

    fn edit_selection(
        clip_ids: &[ClipId],
        time: Option<(notwig_core::RangeMus, Vec<TrackId>)>,
    ) -> edit::Selection<TrackId, ClipId> {
        if let Some((range, tracks)) = time {
            edit::Selection::time(range, tracks)
        } else {
            edit::Selection::objects(clip_ids.iter().copied())
        }
    }

    fn lower_and_apply(
        &mut self,
        plan: edit::Plan<TrackId, ClipId, ClipContent>,
    ) -> Result<Option<SelectionInfo<TrackId, ClipId>>, String> {
        let lowered = { lower_arranger(ArrangerModel::new(&self.doc), plan)? };
        if !lowered.commands.is_empty() {
            self.apply_batch(lowered.commands)?;
        }
        Ok(lowered.selection)
    }

    pub fn apply_batch(&mut self, payloads: Vec<CommandPayload>) -> Result<(), String> {
        if payloads.is_empty() {
            return Ok(());
        }
        let before = self.doc.clone();
        let mut doc = self.doc.clone();
        let mut structural = false;
        let mut engine_msgs = Vec::new();
        for payload in payloads {
            structural |= payload.requires_plan_rebuild();
            let result = apply(&doc, &Command::new(payload)).map_err(|err| err.to_string())?;
            doc = result.doc;
            engine_msgs.extend(result.engine_msgs);
        }
        self.doc = doc;
        self.dirty = true;
        self.history.record(before, structural);
        if structural {
            self.request_engine_rebuild();
        } else {
            let (transport, all_notes_off) = control_updates(engine_msgs);
            self.publish_engine_control(transport, all_notes_off, None);
        }
        self.error = None;
        Ok(())
    }

    pub fn copy_selection(
        &mut self,
        clip_ids: &[ClipId],
        time: Option<(notwig_core::RangeMus, Vec<TrackId>)>,
    ) -> usize {
        let selection = Self::edit_selection(clip_ids, time);
        let result = {
            let model = ArrangerModel::new(&self.doc);
            edit::copy(&model, &selection)
        };
        match result {
            Ok(extract) => {
                let count = extract.fragments.len();
                self.clipboard = Clipboard::Arranger(extract);
                self.status = Some("Copied selection.".into());
                self.error = None;
                count
            }
            Err(err) => {
                self.error = Some(err.to_string());
                0
            }
        }
    }

    #[doc(hidden)]
    pub fn copy_clips(&mut self, clip_ids: &[ClipId]) -> usize {
        self.copy_selection(clip_ids, None)
    }

    #[doc(hidden)]
    pub fn copy_time_range(&mut self, range: notwig_core::RangeMus, tracks: &[TrackId]) -> usize {
        self.copy_selection(&[], Some((range, tracks.to_vec())))
    }

    pub fn cut_selection(
        &mut self,
        clip_ids: &[ClipId],
        time: Option<(notwig_core::RangeMus, Vec<TrackId>)>,
    ) -> usize {
        let selection = Self::edit_selection(clip_ids, time);
        let pair = {
            let model = ArrangerModel::new(&self.doc);
            edit::cut(&model, &selection)
        };
        let (extract, plan) = match pair {
            Ok(pair) => pair,
            Err(err) => {
                self.error = Some(err.to_string());
                return 0;
            }
        };
        let count = extract.fragments.len();
        if let Err(err) = self.lower_and_apply(plan) {
            self.error = Some(err);
            return 0;
        }
        self.clipboard = Clipboard::Arranger(extract);
        self.status = Some("Cut selection.".into());
        count
    }

    pub fn paste_clips(
        &mut self,
        at: notwig_core::TimeMus,
    ) -> Result<SelectionInfo<TrackId, ClipId>, String> {
        let extract = match &self.clipboard {
            Clipboard::Arranger(extract) => extract.clone(),
            Clipboard::Empty => return Ok(SelectionInfo::default()),
            Clipboard::AudioClip(_) => {
                return Err("Clipboard belongs to another editor.".into());
            }
        };
        let plan = {
            let model = ArrangerModel::new(&self.doc);
            edit::paste(&model, &extract, at).map_err(|err| err.to_string())?
        };
        let selection = self.lower_and_apply(plan)?.unwrap_or_default();
        self.status = Some("Pasted selection.".into());
        Ok(selection)
    }

    #[must_use]
    pub fn clipboard(&self) -> &Clipboard {
        &self.clipboard
    }

    pub fn set_clipboard(&mut self, clipboard: Clipboard) {
        self.clipboard = clipboard;
    }

    pub fn delete_selection(
        &mut self,
        clip_ids: &[ClipId],
        time: Option<(notwig_core::RangeMus, Vec<TrackId>)>,
    ) -> Result<SelectionInfo<TrackId, ClipId>, String> {
        let selection = Self::edit_selection(clip_ids, time);
        let plan = {
            let model = ArrangerModel::new(&self.doc);
            edit::delete(&model, &selection).map_err(|err| err.to_string())?
        };
        self.lower_and_apply(plan).map(Option::unwrap_or_default)
    }

    #[doc(hidden)]
    pub fn delete_time_range(
        &mut self,
        range: notwig_core::RangeMus,
        tracks: &[TrackId],
    ) -> Result<(), String> {
        self.delete_selection(&[], Some((range, tracks.to_vec())))?;
        self.status = Some("Deleted time selection.".into());
        Ok(())
    }

    pub fn duplicate_selection(
        &mut self,
        clip_ids: &[ClipId],
        time: Option<(notwig_core::RangeMus, Vec<TrackId>)>,
    ) -> Result<SelectionInfo<TrackId, ClipId>, String> {
        let selection = Self::edit_selection(clip_ids, time);
        let plan = {
            let model = ArrangerModel::new(&self.doc);
            edit::duplicate(&model, &selection).map_err(|err| err.to_string())?
        };
        self.lower_and_apply(plan).map(Option::unwrap_or_default)
    }

    #[doc(hidden)]
    pub fn duplicate_time_range(
        &mut self,
        range: notwig_core::RangeMus,
        tracks: &[TrackId],
    ) -> Result<(), String> {
        self.duplicate_selection(&[], Some((range, tracks.to_vec())))?;
        Ok(())
    }

    pub fn move_time_selection(
        &mut self,
        range: notwig_core::RangeMus,
        tracks: &[TrackId],
        delta: notwig_core::TimeMus,
    ) -> Result<SelectionInfo<TrackId, ClipId>, String> {
        let selection = edit::Selection::time(range, tracks.iter().copied());
        let plan = {
            let model = ArrangerModel::new(&self.doc);
            edit::move_selection(&model, &selection, delta).map_err(|err| err.to_string())?
        };
        self.lower_and_apply(plan).map(Option::unwrap_or_default)
    }

    pub fn render_to(&self, path: &Path) -> Result<usize, String> {
        let end_beat = self
            .doc
            .arranger
            .clips
            .values()
            .map(|clip| clip.range.end)
            .max()
            .unwrap_or_else(|| Rational::new(4, 1));
        self.render_range_to(
            path,
            notwig_core::TimeMus::zero(),
            end_beat,
            notwig_project::ExportOptions::default(),
        )
    }

    pub fn render_range_to(
        &self,
        path: &Path,
        start: notwig_core::TimeMus,
        end: notwig_core::TimeMus,
        options: notwig_project::ExportOptions,
    ) -> Result<usize, String> {
        let mut session = self.begin_render(start, end)?;
        let total = session.total_frames();
        let mut output = Vec::with_capacity(total * 2);
        while output.len() / 2 < total {
            let frames = (total - output.len() / 2).min(4096);
            output.extend(session.render_block(frames)?);
        }
        output.truncate(total * 2);
        notwig_project::write_audio_export(path, session.sample_rate, 2, &output, options)
            .map_err(|err| err.to_string())?;
        Ok(output.len() / 2)
    }

    pub fn begin_render(
        &self,
        start: notwig_core::TimeMus,
        end: notwig_core::TimeMus,
    ) -> Result<RenderSession, String> {
        let map = self.doc.tempo_map();
        let start_sample = map.mus_to_samples(start.max(notwig_core::TimeMus::zero()));
        let end_sample = map
            .mus_to_samples(end.max(start + notwig_core::TimeMus::new(1, 16)))
            .max(start_sample + 1);
        let mut engine = self.build_engine()?;
        engine
            .apply(EngineMsg::SetTransport {
                playing: true,
                playhead_samples: start_sample,
                loop_enabled: false,
                loop_start_samples: None,
                loop_end_samples: None,
                stop: false,
                metronome: None,
            })
            .map_err(|err| err.to_string())?;
        engine.begin_offline_render(start_sample);
        Ok(RenderSession {
            engine,
            start_sample,
            end_sample,
            sample_rate: self.doc.meta.sample_rate.value(),
        })
    }

    pub fn render_stems_to(
        &self,
        directory: &Path,
        start: notwig_core::TimeMus,
        end: notwig_core::TimeMus,
        options: notwig_project::ExportOptions,
    ) -> Result<Vec<(PathBuf, usize)>, String> {
        std::fs::create_dir_all(directory).map_err(|err| err.to_string())?;
        let map = self.doc.tempo_map();
        let start_sample = map.mus_to_samples(start.max(notwig_core::TimeMus::zero()));
        let end_sample = map
            .mus_to_samples(end.max(start + notwig_core::TimeMus::new(1, 16)))
            .max(start_sample + 1);
        let master = self.doc.master_track();
        let mut written = Vec::new();
        for track in self
            .doc
            .tracks
            .values()
            .filter(|track| track.kind != notwig_core::TrackKind::Master)
        {
            let mut engine = self.build_engine()?;
            engine
                .apply(EngineMsg::SetTransport {
                    playing: true,
                    playhead_samples: start_sample,
                    loop_enabled: false,
                    loop_start_samples: None,
                    loop_end_samples: None,
                    stop: false,
                    metronome: None,
                })
                .map_err(|err| err.to_string())?;
            for state in self.doc.tracks.values() {
                let ancestor = is_ancestor(&self.doc, state.id, track.id);
                let audible = state.id == track.id || ancestor || Some(state.id) == master;
                engine
                    .apply(EngineMsg::SetTrackState {
                        track_id: state.id,
                        mute: state.mixer.mute,
                        solo_effective: audible,
                        gain_db: state.mixer.volume_db,
                        pan: state.mixer.pan,
                    })
                    .map_err(|err| err.to_string())?;
            }
            let output = engine
                .render_offline(start_sample, end_sample)
                .map_err(|err| err.to_string())?;
            let name = sanitize_file_name(&track.name);
            let path = directory.join(format!("{name}.{}", options.format.extension()));
            notwig_project::write_audio_export(
                &path,
                self.doc.meta.sample_rate.value(),
                2,
                &output,
                options,
            )
            .map_err(|err| err.to_string())?;
            written.push((path, output.len() / 2));
        }
        Ok(written)
    }
}

fn is_ancestor(
    doc: &ProjectDoc,
    candidate: notwig_core::TrackId,
    track: notwig_core::TrackId,
) -> bool {
    let mut current = doc.tracks.get(&track).and_then(|track| track.parent);
    while let Some(parent) = current {
        if parent == candidate {
            return true;
        }
        current = doc.tracks.get(&parent).and_then(|track| track.parent);
    }
    false
}

fn sanitize_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ' ') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "stem".into()
    } else {
        trimmed.to_string()
    }
}

fn control_updates(messages: Vec<EngineMsg>) -> (Option<TransportUpdate>, bool) {
    let mut transport = None;
    let mut all_notes_off = false;
    for message in messages {
        match message {
            EngineMsg::SetTransport {
                playing,
                playhead_samples,
                loop_enabled,
                loop_start_samples,
                loop_end_samples,
                stop,
                metronome,
            } => {
                transport = Some(TransportUpdate {
                    playing,
                    playhead_samples,
                    loop_enabled,
                    loop_start_samples,
                    loop_end_samples,
                    stop,
                    metronome,
                });
            }
            EngineMsg::AllNotesOff => all_notes_off = true,
            EngineMsg::ApplyPlan { .. }
            | EngineMsg::SetParam { .. }
            | EngineMsg::SetTrackState { .. }
            | EngineMsg::UpdateSchedule { .. }
            | EngineMsg::PrepareAudioAsset { .. } => {}
        }
    }
    (transport, all_notes_off)
}

impl Drop for Workspace {
    fn drop(&mut self) {
        for dir in self.pending_import_dirs.values() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

fn scale_sample(value: u64, ratio: f64) -> u64 {
    (value as f64 * ratio).round() as u64
}

pub struct RenderSession {
    engine: Engine,
    pub start_sample: u64,
    pub end_sample: u64,
    pub sample_rate: u32,
}

impl RenderSession {
    #[must_use]
    pub fn total_frames(&self) -> usize {
        usize::try_from(self.end_sample.saturating_sub(self.start_sample)).unwrap_or(0)
    }

    pub fn render_block(&mut self, frames: usize) -> Result<Vec<f32>, String> {
        self.engine
            .render_offline_block(frames)
            .map_err(|err| err.to_string())
    }
}
