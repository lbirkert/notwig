use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, mpsc};

use crate::history::History;
use notwig_core::ids::{
    Clip as ClipMarker, Device as DeviceMarker, DeviceChain as DeviceChainMarker,
    Note as NoteMarker, Track as TrackMarker,
};
use notwig_core::protocol::{Command, CommandPayload, EngineMsg, PlanHandle, PlaybackSchedule};
use notwig_core::sequencer::MidiClipData;
use notwig_core::{
    AssetId, AudioAssetBuffer, ClipId, ClipPayload, Db, DeviceChainId, DeviceId, DeviceKind, Id,
    Meta, ProjectDoc, Rational, SampleRate, SchemaVersion, TrackId, TrackKind, apply,
};
use notwig_dsp::EngineCaps;
use notwig_engine::{Engine, EngineConfig, PluginDeviceFactory};
use notwig_plugins::{PluginCache, PluginDescriptor, PluginHost};
use notwig_ui::PluginEntry;
use uuid::Uuid;

use crate::edit_lowering::lower_arranger;
use notwig_core::edit::{self, ArrangerModel, ClipContent, Extract, SelectionInfo};

#[derive(Debug, Clone, Default)]
struct Clipboard {
    extract: Option<Extract<TrackId, ClipContent>>,
}

pub struct Workspace {
    pub doc: ProjectDoc,
    pub bundle: Option<PathBuf>,
    pub dirty: bool,
    pub assets: BTreeMap<AssetId, Arc<AudioAssetBuffer>>,
    pub peaks: BTreeMap<AssetId, Vec<f32>>,
    pub engine: Arc<Mutex<Engine>>,
    pub plugins: Vec<PluginEntry>,
    plugin_descriptors: Vec<PluginDescriptor>,
    plugin_specs: BTreeMap<String, notwig_dsp::DeviceSpec>,
    pub status: Option<String>,
    pub error: Option<String>,
    pub last_autosave: std::time::Instant,
    pub play_start: Option<notwig_core::TimeMus>,
    pub audio_block_size: u32,
    clipboard: Clipboard,
    history: History,
    engine_build: Option<mpsc::Receiver<Result<Engine, String>>>,
    engine_rebuild_queued: bool,
    engine_build_requests: u64,
}

struct EngineBuildRequest {
    doc: ProjectDoc,
    caps: EngineCaps,
    registry: notwig_dsp::DeviceRegistry,
    plugin_descriptors: Vec<PluginDescriptor>,
    assets: BTreeMap<AssetId, Arc<AudioAssetBuffer>>,
    sample_rate: u32,
    max_block_size: usize,
}

fn build_engine_from_request(request: EngineBuildRequest) -> Result<Engine, String> {
    let EngineBuildRequest {
        doc,
        caps,
        registry,
        plugin_descriptors,
        assets,
        sample_rate,
        max_block_size,
    } = request;
    let plan = notwig_dsp::compile(&doc, &registry, caps).map_err(|err| err.to_string())?;
    let config = EngineConfig::new(sample_rate, max_block_size);
    let factory = Arc::new(PluginDeviceFactory::new(
        new_plugin_host(),
        plugin_descriptors,
    ));
    let mut engine = Engine::with_factory(config, factory);
    engine
        .apply(EngineMsg::UpdateSchedule {
            schedule: Arc::new(PlaybackSchedule::from_doc(&doc)),
        })
        .map_err(|err| err.to_string())?;
    engine
        .apply(EngineMsg::ApplyPlan {
            plan: PlanHandle::new(plan),
        })
        .map_err(|err| err.to_string())?;
    for (asset_id, buffer) in &assets {
        engine
            .apply(EngineMsg::PrepareAudioAsset {
                asset_id: *asset_id,
                buffer: Arc::clone(buffer),
            })
            .map_err(|err| err.to_string())?;
    }
    Ok(engine)
}

impl Workspace {
    pub fn new_empty() -> Result<Self, String> {
        Self::new_empty_at(48_000)
    }

    pub fn new_empty_at(sample_rate: u32) -> Result<Self, String> {
        let meta = Meta {
            name: "Untitled".into(),
            created_utc: 0,
            modified_utc: 0,
            sample_rate: SampleRate::new(sample_rate).map_err(|err| err.to_string())?,
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
        if loaded.recovery_offered {
            workspace.status = Some("Recovery autosaves are available.".into());
        }
        Ok(workspace)
    }

    fn from_doc(doc: ProjectDoc, bundle: Option<PathBuf>, dirty: bool) -> Self {
        let sample_rate = doc.meta.sample_rate.value();
        let plugin_descriptors = PluginCatalog::global().descriptors();
        let plugins = plugin_entries(&plugin_descriptors);
        let engine = Arc::new(Mutex::new(Engine::new(EngineConfig::new(sample_rate, 512))));
        let mut workspace = Self {
            doc,
            bundle,
            dirty,
            assets: BTreeMap::new(),
            peaks: BTreeMap::new(),
            engine,
            plugins,
            plugin_descriptors,
            plugin_specs: BTreeMap::new(),
            status: None,
            error: None,
            last_autosave: std::time::Instant::now(),
            play_start: None,
            audio_block_size: 512,
            clipboard: Clipboard::default(),
            history: History::default(),
            engine_build: None,
            engine_rebuild_queued: false,
            engine_build_requests: 0,
        };
        workspace.refresh_plugin_specs();
        workspace.load_assets();
        workspace.request_engine_rebuild();
        workspace
    }

    fn load_assets(&mut self) {
        self.assets.clear();
        self.peaks.clear();
        let Some(bundle) = self.bundle.clone() else {
            return;
        };
        for entry in self.doc.assets.assets.values() {
            if entry.kind != notwig_core::AssetKind::Audio {
                continue;
            }
            let path = bundle.join(&entry.uri);
            match notwig_project::decode_audio(&path) {
                Ok(buffer) => {
                    self.peaks.insert(entry.id, compute_peaks(&buffer, 2048));
                    self.assets.insert(entry.id, Arc::new(buffer));
                }
                Err(err) => {
                    self.status = Some(format!("Asset {} unavailable: {err}", entry.id));
                }
            }
        }
    }

    #[allow(dead_code)]
    pub fn rebuild_engine(&mut self) -> Result<(), String> {
        let preserved = self.preserved_playhead_samples();
        let mut engine = self.build_engine()?;
        self.apply_transport(&mut engine, preserved)?;
        *self.engine.lock().map_err(|err| err.to_string())? = engine;
        Ok(())
    }

    fn preserved_playhead_samples(&self) -> u64 {
        self.engine.lock().ok().map_or_else(
            || {
                self.doc
                    .tempo_map()
                    .mus_to_samples(self.doc.transport.playhead)
            },
            |engine| engine.playhead_samples(),
        )
    }

    fn apply_transport(&self, engine: &mut Engine, preserved: u64) -> Result<(), String> {
        let map = self.doc.tempo_map();
        let (loop_start, loop_end) = self.doc.transport.loop_range.map_or((None, None), |range| {
            (
                Some(map.mus_to_samples(range.start)),
                Some(map.mus_to_samples(range.end)),
            )
        });
        engine
            .apply(EngineMsg::SetTransport {
                playing: self.doc.transport.playing,
                playhead_samples: preserved,
                loop_enabled: self.doc.transport.loop_enabled,
                loop_start_samples: loop_start,
                loop_end_samples: loop_end,
                stop: false,
            })
            .map_err(|err| err.to_string())
    }

    fn build_engine(&self) -> Result<Engine, String> {
        build_engine_from_request(self.engine_build_request())
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
            assets: self.assets.clone(),
            sample_rate,
            max_block_size,
        }
    }

    pub fn request_engine_rebuild(&mut self) {
        self.engine_build_requests = self.engine_build_requests.saturating_add(1);
        self.status = Some("Loading instruments and effects…".into());
        if self.engine_build.is_some() {
            self.engine_rebuild_queued = true;
            return;
        }
        self.spawn_engine_build();
    }

    fn spawn_engine_build(&mut self) {
        let request = self.engine_build_request();
        let (sender, receiver) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("notwig-engine-build".into())
            .spawn(move || {
                let _ = sender.send(build_engine_from_request(request));
            });
        match spawned {
            Ok(_) => self.engine_build = Some(receiver),
            Err(err) => {
                self.engine_build = None;
                self.error = Some(format!("Could not start engine build: {err}"));
                self.status = self.error.clone();
            }
        }
    }

    pub fn engine_building(&self) -> bool {
        self.engine_build.is_some() || self.engine_rebuild_queued
    }

    #[allow(dead_code)]
    pub fn engine_build_pending(&self) -> bool {
        self.engine_building()
    }

    #[allow(dead_code)]
    #[must_use]
    pub fn engine_build_requests(&self) -> u64 {
        self.engine_build_requests
    }

    pub fn poll_engine_build(&mut self) -> bool {
        let outcome = match self.engine_build.as_ref() {
            Some(receiver) => receiver.try_recv(),
            None => return false,
        };
        match outcome {
            Ok(Ok(engine)) => {
                self.install_built_engine(engine);
                let queued = self.engine_rebuild_queued;
                self.engine_build = None;
                self.engine_rebuild_queued = false;
                self.status = Some("Instruments and effects loaded.".into());
                if queued {
                    self.spawn_engine_build();
                }
                true
            }
            Ok(Err(err)) => {
                let queued = self.engine_rebuild_queued;
                self.engine_build = None;
                self.engine_rebuild_queued = false;
                self.error = Some(err.clone());
                self.status = Some(format!("Failed to load instruments and effects: {err}"));
                if queued {
                    self.spawn_engine_build();
                }
                true
            }
            Err(mpsc::TryRecvError::Empty) => false,
            Err(mpsc::TryRecvError::Disconnected) => {
                let queued = self.engine_rebuild_queued;
                self.engine_build = None;
                self.engine_rebuild_queued = false;
                self.error = Some("Engine build thread disconnected.".into());
                self.status = self.error.clone();
                if queued {
                    self.spawn_engine_build();
                }
                true
            }
        }
    }

    fn install_built_engine(&mut self, mut engine: Engine) {
        let preserved = self.preserved_playhead_samples();
        if let Err(err) = self.apply_transport(&mut engine, preserved) {
            self.error = Some(err);
        }
        let old = match self.engine.lock() {
            Ok(mut slot) => std::mem::replace(&mut *slot, engine),
            Err(err) => {
                self.error = Some(err.to_string());
                return;
            }
        };
        drop(old);
        self.sync_engine();
    }

    fn sync_engine(&mut self) {
        let schedule = Arc::new(PlaybackSchedule::from_doc(&self.doc));
        let track_states: Vec<EngineMsg> = self
            .doc
            .tracks
            .values()
            .map(|track| EngineMsg::SetTrackState {
                track_id: track.id,
                mute: track.mixer.mute,
                solo_effective: self.doc.effective_solo(track.id),
                gain_db: track.mixer.volume_db,
                pan: track.mixer.pan,
            })
            .collect();
        let mut messages = track_states;
        for device in self.doc.devices.values() {
            for (param_id, value) in &device.param_values {
                messages.push(EngineMsg::SetParam {
                    device_id: device.id,
                    param_id: *param_id,
                    value: value.clone(),
                    ramp: None,
                });
            }
        }
        let Ok(mut engine) = self.engine.lock() else {
            return;
        };
        if engine
            .apply(EngineMsg::UpdateSchedule { schedule })
            .is_err()
        {
            return;
        }
        for message in messages {
            if engine.apply(message).is_err() {
                break;
            }
        }
    }

    pub fn apply(&mut self, payload: CommandPayload) -> Result<(), String> {
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
                } else if let Ok(mut engine) = self.engine.lock() {
                    for message in result.engine_msgs {
                        if engine.apply(message).is_err() {
                            break;
                        }
                    }
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
        notwig_project::save(&self.doc, &path).map_err(|err| err.to_string())?;
        self.bundle = Some(path.clone());
        self.dirty = false;
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
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| {
                i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
            });
        let _ = notwig_project::autosave(&self.doc, &bundle, timestamp);
        self.last_autosave = std::time::Instant::now();
    }

    pub fn set_sample_rate(&mut self, sample_rate: u32) -> Result<(), String> {
        let rate = notwig_core::SampleRate::new(sample_rate).map_err(|err| err.to_string())?;
        if self.doc.meta.sample_rate == rate {
            return Ok(());
        }
        let before = self.doc.clone();
        self.doc.meta.sample_rate = rate;
        self.dirty = true;
        self.history.record(before, true);
        self.request_engine_rebuild();
        Ok(())
    }

    pub fn set_playhead(&mut self, playhead: notwig_core::TimeMus) -> Result<(), String> {
        self.apply(CommandPayload::SetTransport {
            playing: None,
            playhead: Some(playhead),
            loop_enabled: None,
            loop_range: None,
        })
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
        tracing::info!(
            plugin = %descriptor.name,
            params = params.len(),
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
                latency_samples: 0,
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

    fn prepare_asset(&self, asset_id: AssetId, buffer: Arc<AudioAssetBuffer>) {
        if let Ok(mut engine) = self.engine.lock() {
            let _ = engine.apply(EngineMsg::PrepareAudioAsset { asset_id, buffer });
        }
    }

    pub fn import_audio(&mut self, source: &Path) -> Result<AssetId, String> {
        let bundle = self
            .bundle
            .clone()
            .ok_or_else(|| "save the project before importing audio".to_string())?;
        let asset_id = Id::<notwig_core::ids::Asset>::from_uuid(Uuid::new_v4());
        let imported = notwig_project::import_asset(
            source,
            &bundle,
            asset_id,
            self.doc.meta.sample_rate.value(),
        )
        .map_err(|err| err.to_string())?;
        self.peaks
            .insert(asset_id, compute_peaks(&imported.buffer, 2048));
        let buffer = Arc::new(imported.buffer);
        self.assets.insert(asset_id, Arc::clone(&buffer));
        self.apply(CommandPayload::RegisterAsset {
            asset_id,
            entry: imported.entry,
        })?;
        self.prepare_asset(asset_id, buffer);
        self.status = Some(format!("Imported {}", source.display()));
        Ok(asset_id)
    }

    pub fn import_audio_at(
        &mut self,
        source: &Path,
        track_id: TrackId,
        start: notwig_core::TimeMus,
    ) -> Result<(), String> {
        let asset_id = self.import_audio(source)?;
        self.insert_audio_clip_at(track_id, asset_id, start)
    }

    pub fn import_audio_slice(
        &mut self,
        source: &Path,
        clip_id: ClipId,
        at: notwig_core::TimeMus,
    ) -> Result<(), String> {
        let asset_id = self.import_audio(source)?;
        self.add_audio_slice(clip_id, asset_id, at)
    }

    pub fn add_audio_slice(
        &mut self,
        clip_id: ClipId,
        asset_id: AssetId,
        at: notwig_core::TimeMus,
    ) -> Result<(), String> {
        let clip = self
            .doc
            .arranger
            .clips
            .get(&clip_id)
            .cloned()
            .ok_or_else(|| "clip not found".to_string())?;
        let ClipPayload::Audio(audio) = &clip.payload else {
            return Err("clip is not an audio clip".into());
        };
        let duration = self
            .doc
            .assets
            .assets
            .get(&asset_id)
            .ok_or_else(|| "asset not found".to_string())?
            .duration_samples
            .ok_or_else(|| "asset has no duration".to_string())?;
        let entering_slice_mode = !audio.uses_slices();
        let mut slices = if entering_slice_mode {
            vec![notwig_core::sequencer::AudioSlice {
                id: notwig_core::SliceId::from_uuid(Uuid::new_v4()),
                start: notwig_core::TimeMus::zero(),
                source_start_sample: audio.source_start_sample,
                source_end_sample: audio.source_end_sample,
                gain_db: audio.gain_db,
                asset_id: None,
            }]
        } else {
            audio.slices.clone()
        };
        if entering_slice_mode {
            self.apply(CommandPayload::SetClipSlices {
                clip_id,
                slices: slices.clone(),
            })?;
        }
        let start = at.max(notwig_core::TimeMus::zero());
        slices.push(notwig_core::sequencer::AudioSlice {
            id: notwig_core::SliceId::from_uuid(Uuid::new_v4()),
            start,
            source_start_sample: 0,
            source_end_sample: duration,
            gain_db: Db(0.0),
            asset_id: Some(asset_id),
        });
        self.apply(CommandPayload::SetClipSlices { clip_id, slices })?;
        self.status = Some("Added audio event.".into());
        Ok(())
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
        self.apply(CommandPayload::CreateClip {
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
        })
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
        } else if let Ok(mut engine) = self.engine.lock() {
            for message in engine_msgs {
                if engine.apply(message).is_err() {
                    break;
                }
            }
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
                self.clipboard.extract = Some(extract);
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
        self.clipboard.extract = Some(extract);
        self.status = Some("Cut selection.".into());
        count
    }

    pub fn paste_clips(
        &mut self,
        at: notwig_core::TimeMus,
    ) -> Result<SelectionInfo<TrackId, ClipId>, String> {
        let Some(extract) = self.clipboard.extract.clone() else {
            return Ok(SelectionInfo::default());
        };
        let plan = {
            let model = ArrangerModel::new(&self.doc);
            edit::paste(&model, &extract, at).map_err(|err| err.to_string())?
        };
        let selection = self.lower_and_apply(plan)?.unwrap_or_default();
        self.status = Some("Pasted selection.".into());
        Ok(selection)
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

    pub fn relink_asset(&mut self, asset_id: AssetId, path: &Path) -> Result<(), String> {
        let uri = path.to_string_lossy().into_owned();
        self.apply(CommandPayload::RelinkAsset { asset_id, uri })?;
        match notwig_project::decode_audio(path) {
            Ok(buffer) => {
                self.peaks.insert(asset_id, compute_peaks(&buffer, 2048));
                let buffer = Arc::new(buffer);
                self.assets.insert(asset_id, Arc::clone(&buffer));
                self.prepare_asset(asset_id, buffer);
                self.status = Some(format!("Relinked asset to {}", path.display()));
                Ok(())
            }
            Err(err) => {
                self.error = Some(err.to_string());
                Err(err.to_string())
            }
        }
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
        self.render_range_to(path, notwig_core::TimeMus::zero(), end_beat)
    }

    pub fn render_range_to(
        &self,
        path: &Path,
        start: notwig_core::TimeMus,
        end: notwig_core::TimeMus,
    ) -> Result<usize, String> {
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
            })
            .map_err(|err| err.to_string())?;
        let output = engine
            .render_offline(start_sample, end_sample)
            .map_err(|err| err.to_string())?;
        notwig_project::write_wav_f32(path, self.doc.meta.sample_rate.value(), 2, &output)
            .map_err(|err| err.to_string())?;
        Ok(output.len() / 2)
    }
}

fn compute_peaks(buffer: &AudioAssetBuffer, buckets: usize) -> Vec<f32> {
    let frames = buffer.frame_count().max(1) as usize;
    let channels = usize::from(buffer.channels.max(1));
    let mut peaks = vec![0.0_f32; buckets.max(1)];
    for index in 0..frames {
        let bucket = index * peaks.len() / frames;
        let mut peak = 0.0_f32;
        for channel in 0..channels {
            if let Some(sample) = buffer.samples.get(index * channels + channel) {
                peak = peak.max(sample.abs());
            }
        }
        peaks[bucket] = peaks[bucket].max(peak);
    }
    peaks
}

fn new_plugin_host() -> PluginHost {
    PluginHost::new()
}

struct PluginCatalog {
    descriptors: Mutex<Vec<PluginDescriptor>>,
    scan: Mutex<Option<mpsc::Receiver<Vec<PluginDescriptor>>>>,
}

impl PluginCatalog {
    fn global() -> &'static Self {
        static CATALOG: OnceLock<PluginCatalog> = OnceLock::new();
        CATALOG.get_or_init(|| {
            if cfg!(test) {
                return Self {
                    descriptors: Mutex::new(Vec::new()),
                    scan: Mutex::new(None),
                };
            }
            let cache = plugin_cache_path()
                .and_then(|path| PluginCache::load(&path).ok())
                .map(|cache| cache.descriptors())
                .unwrap_or_default();
            tracing::info!(count = cache.len(), "loaded cached audio plugin registry");
            let (tx, rx) = mpsc::channel();
            let cache_path = plugin_cache_path();
            std::thread::spawn(move || {
                if let Some(path) = cache_path {
                    if let Ok(exe) = std::env::current_exe() {
                        let status = std::process::Command::new(exe)
                            .arg("scan-plugins")
                            .arg(&path)
                            .status();
                        if !status.is_ok_and(|status| status.success()) {
                            tracing::warn!("plugin scanner process failed");
                        }
                    }
                    let descriptors = PluginCache::load(&path)
                        .ok()
                        .map(|cache| cache.descriptors())
                        .unwrap_or_default();
                    tracing::info!(count = descriptors.len(), "scanned audio plugins");
                    let _ = tx.send(descriptors);
                }
            });
            Self {
                descriptors: Mutex::new(cache),
                scan: Mutex::new(Some(rx)),
            }
        })
    }

    fn descriptors(&self) -> Vec<PluginDescriptor> {
        self.descriptors
            .lock()
            .map_or_else(|_| Vec::new(), |descriptors| descriptors.clone())
    }

    fn poll(&self) -> bool {
        let Ok(mut scan) = self.scan.lock() else {
            return false;
        };
        let Some(receiver) = scan.as_ref() else {
            return false;
        };
        match receiver.try_recv() {
            Ok(descriptors) => {
                if let Ok(mut current) = self.descriptors.lock() {
                    *current = descriptors;
                }
                *scan = None;
                true
            }
            Err(mpsc::TryRecvError::Empty) => false,
            Err(mpsc::TryRecvError::Disconnected) => {
                *scan = None;
                false
            }
        }
    }
}

fn plugin_cache_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let home = PathBuf::from(home);
    #[cfg(target_os = "macos")]
    let path = home.join("Library/Caches/notwig/plugin-cache.json");
    #[cfg(not(target_os = "macos"))]
    let path = home.join(".cache/notwig/plugin-cache.json");
    Some(path)
}

fn plugin_entries(descriptors: &[PluginDescriptor]) -> Vec<PluginEntry> {
    descriptors
        .iter()
        .map(|descriptor| PluginEntry {
            type_id: descriptor.type_id.clone(),
            name: descriptor.name.clone(),
            vendor: descriptor.vendor.clone(),
            is_instrument: descriptor.is_instrument,
            kind: if descriptor.is_instrument {
                DeviceKind::Instrument
            } else {
                DeviceKind::AudioFx
            },
        })
        .collect()
}
