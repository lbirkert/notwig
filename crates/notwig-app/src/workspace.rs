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
use notwig_ui::{ClipboardMode, PluginEntry};
use uuid::Uuid;

#[derive(Debug, Clone)]
struct ClipboardEntry {
    track_id: TrackId,
    offset: notwig_core::TimeMus,
    clip: notwig_core::ClipEntity,
}

#[derive(Debug, Clone, Default)]
struct Clipboard {
    clips: Vec<ClipboardEntry>,
    mode: ClipboardMode,
    time_range: Option<notwig_core::RangeMus>,
    time_tracks: Vec<TrackId>,
}

#[derive(Debug, Clone, Default)]
pub struct PasteResult {
    pub clip_ids: Vec<ClipId>,
    pub clip_tracks: Vec<(ClipId, TrackId)>,
    pub time: Option<(notwig_core::RangeMus, Vec<TrackId>)>,
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
    clipboard: Clipboard,
    history: History,
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
        Self::from_doc(doc, None, true)
    }

    pub fn open(bundle: &Path) -> Result<Self, String> {
        let loaded = notwig_project::open(bundle).map_err(|err| err.to_string())?;
        let mut workspace = Self::from_doc(loaded.doc, Some(bundle.to_path_buf()), false)?;
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

    fn from_doc(doc: ProjectDoc, bundle: Option<PathBuf>, dirty: bool) -> Result<Self, String> {
        let plugin_descriptors = PluginCatalog::global().descriptors();
        let plugins = plugin_entries(&plugin_descriptors);
        let engine = Arc::new(Mutex::new(Engine::new(EngineConfig::default())));
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
            clipboard: Clipboard::default(),
            history: History::default(),
        };
        workspace.refresh_plugin_specs();
        workspace.load_assets();
        workspace.rebuild_engine()?;
        Ok(workspace)
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

    pub fn rebuild_engine(&mut self) -> Result<(), String> {
        let preserved = self.engine.lock().ok().map_or_else(
            || {
                self.doc
                    .tempo_map()
                    .mus_to_samples(self.doc.transport.playhead)
            },
            |engine| engine.playhead_samples(),
        );
        let mut engine = self.build_engine()?;
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
            .map_err(|err| err.to_string())?;
        *self.engine.lock().map_err(|err| err.to_string())? = engine;
        Ok(())
    }

    fn build_engine(&self) -> Result<Engine, String> {
        let caps = EngineCaps {
            sample_rate: self.doc.meta.sample_rate.value(),
            max_block_size: 512,
            channels: 2,
        };
        let registry = self.device_registry();
        let plan =
            notwig_dsp::compile(&self.doc, &registry, caps).map_err(|err| err.to_string())?;
        let config = EngineConfig::new(self.doc.meta.sample_rate.value(), 512);
        let factory = Arc::new(PluginDeviceFactory::new(
            new_plugin_host(),
            self.plugin_descriptors.clone(),
        ));
        let mut engine = Engine::with_factory(config, factory);
        engine
            .apply(EngineMsg::UpdateSchedule {
                schedule: Arc::new(PlaybackSchedule::from_doc(&self.doc)),
            })
            .map_err(|err| err.to_string())?;
        engine
            .apply(EngineMsg::ApplyPlan {
                plan: PlanHandle::new(plan),
            })
            .map_err(|err| err.to_string())?;
        for (asset_id, buffer) in &self.assets {
            engine
                .apply(EngineMsg::PrepareAudioAsset {
                    asset_id: *asset_id,
                    buffer: Arc::clone(buffer),
                })
                .map_err(|err| err.to_string())?;
        }
        Ok(engine)
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
                    self.history.record(before);
                }
                if structural {
                    self.rebuild_engine()?;
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
        let Some(previous) = self.history.undo(&self.doc) else {
            return Ok(());
        };
        self.doc = previous;
        self.dirty = true;
        self.rebuild_engine()?;
        self.status = Some("Undo.".into());
        Ok(())
    }

    pub fn redo(&mut self) -> Result<(), String> {
        let Some(next) = self.history.redo(&self.doc) else {
            return Ok(());
        };
        self.doc = next;
        self.dirty = true;
        self.rebuild_engine()?;
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
        let new_clip = Id::<ClipMarker>::from_uuid(Uuid::new_v4());
        self.duplicate_clip_to(clip_id, new_clip, None)?;
        Ok(new_clip)
    }

    pub fn duplicate_clip_to(
        &mut self,
        clip_id: ClipId,
        new_clip: ClipId,
        range: Option<notwig_core::RangeMus>,
    ) -> Result<(), String> {
        let Some(clip) = self.doc.arranger.clips.get(&clip_id) else {
            return Err("clip not found".into());
        };
        let mut note_map = Vec::new();
        if let ClipPayload::Midi(midi) = &clip.payload {
            for note in &midi.notes {
                note_map.push((note.id, Id::<NoteMarker>::from_uuid(Uuid::new_v4())));
            }
        }
        self.apply(CommandPayload::DuplicateClip {
            clip_id,
            new_clip_id: new_clip,
            note_id_map: note_map,
            track_id: None,
            range,
        })
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
            let _ = self.rebuild_engine();
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
        self.assets.insert(asset_id, Arc::new(imported.buffer));
        self.apply(CommandPayload::RegisterAsset {
            asset_id,
            entry: imported.entry,
        })?;
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
        let length = self.doc.tempo_map().samples_to_mus(duration);
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
        let required_end = start + length;
        if required_end > clip.range.length() {
            self.edit_clip_range(
                clip_id,
                notwig_core::RangeMus::new(clip.range.start, clip.range.start + required_end)
                    .map_err(|err| err.to_string())?,
            )?;
        }
        slices.push(notwig_core::sequencer::AudioSlice {
            id: notwig_core::SliceId::from_uuid(Uuid::new_v4()),
            start,
            source_start_sample: 0,
            source_end_sample: duration,
            gain_db: Db(0.0),
            asset_id: Some(asset_id),
        });
        self.apply(CommandPayload::SetClipSlices { clip_id, slices })?;
        self.status = Some("Added audio slice.".into());
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

    pub fn move_clips(
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
        for (clip_id, track_id, range) in ordered {
            self.apply(CommandPayload::MoveClip {
                clip_id,
                track_id: Some(track_id),
                range,
            })?;
        }
        Ok(())
    }

    pub fn copy_clips(&mut self, clip_ids: &[ClipId]) -> usize {
        let mut clips: Vec<notwig_core::ClipEntity> = clip_ids
            .iter()
            .filter_map(|id| self.doc.arranger.clips.get(id).cloned())
            .collect();
        clips.sort_by_key(|clip| clip.range.start);
        let Some(base) = clips.first().map(|clip| clip.range.start) else {
            return 0;
        };
        self.clipboard.clips = clips
            .into_iter()
            .map(|clip| ClipboardEntry {
                track_id: clip.track_id,
                offset: clip.range.start - base,
                clip,
            })
            .collect();
        self.clipboard.mode = ClipboardMode::Nodes;
        self.clipboard.time_range = None;
        self.clipboard.time_tracks.clear();
        self.status = Some("Copied selection.".into());
        self.clipboard.clips.len()
    }

    pub fn copy_time_range(&mut self, range: notwig_core::RangeMus, tracks: &[TrackId]) -> usize {
        let track_set: std::collections::BTreeSet<TrackId> = tracks.iter().copied().collect();
        self.clipboard.clips = self
            .doc
            .arranger
            .clips
            .values()
            .filter(|clip| track_set.contains(&clip.track_id) && clip.range.intersects(range))
            .filter_map(|clip| {
                self.trim_clip_to_range(clip, range)
                    .map(|trimmed| ClipboardEntry {
                        track_id: trimmed.track_id,
                        offset: trimmed.range.start - range.start,
                        clip: trimmed,
                    })
            })
            .collect();
        self.clipboard.mode = ClipboardMode::Time;
        self.clipboard.time_range = Some(range);
        self.clipboard.time_tracks = tracks.to_vec();
        self.status = Some("Copied time selection.".into());
        self.clipboard.clips.len()
    }

    fn trim_clip_to_range(
        &self,
        clip: &notwig_core::ClipEntity,
        range: notwig_core::RangeMus,
    ) -> Option<notwig_core::ClipEntity> {
        let start = clip.range.start.max(range.start);
        let end = clip.range.end.min(range.end);
        if end <= start {
            return None;
        }
        let mut trimmed = clip.clone();
        trimmed.range = notwig_core::RangeMus::new(start, end).ok()?;
        trimmed.payload = match &clip.payload {
            ClipPayload::Midi(midi) => {
                let mut notes = Vec::new();
                for note in &midi.notes {
                    let absolute_start = clip.range.start + (note.start - midi.content_start);
                    let absolute_end = absolute_start + note.length;
                    let note_start = absolute_start.max(start);
                    let note_end = absolute_end.min(end);
                    if note_end <= note_start {
                        continue;
                    }
                    let mut clipped = note.clone();
                    clipped.start = note_start - start;
                    clipped.length = note_end - note_start;
                    notes.push(clipped);
                }
                ClipPayload::Midi(notwig_core::MidiClipData {
                    length: end - start,
                    content_start: notwig_core::TimeMus::zero(),
                    notes,
                })
            }
            ClipPayload::Audio(audio) => {
                let map = self.doc.tempo_map();
                if audio.uses_slices() {
                    let left_trim = start - clip.range.start;
                    let window_end = left_trim + (end - start);
                    let slices = audio
                        .slices
                        .iter()
                        .filter_map(|slice| {
                            trim_slice_window(map, slice, left_trim, window_end, left_trim)
                        })
                        .collect();
                    ClipPayload::Audio(notwig_core::AudioClipData {
                        asset_id: audio.asset_id,
                        source_start_sample: audio.source_start_sample,
                        source_end_sample: audio.source_end_sample,
                        gain_db: audio.gain_db,
                        fade_in_samples: 0,
                        fade_out_samples: 0,
                        fade_shape: audio.fade_shape,
                        slices,
                        slices_enabled: true,
                    })
                } else {
                    let left = map.samples_delta(start - clip.range.start);
                    let source_start =
                        u64::try_from(i128::from(audio.source_start_sample) + i128::from(left))
                            .ok()?;
                    let duration = map.mus_to_samples(end - start);
                    let source_end = source_start.saturating_add(duration);
                    if source_end <= source_start {
                        return None;
                    }
                    ClipPayload::Audio(notwig_core::AudioClipData {
                        asset_id: audio.asset_id,
                        source_start_sample: source_start,
                        source_end_sample: source_end,
                        gain_db: audio.gain_db,
                        fade_in_samples: 0,
                        fade_out_samples: 0,
                        fade_shape: audio.fade_shape,
                        slices: audio.slices.clone(),
                        slices_enabled: audio.slices_enabled,
                    })
                }
            }
        };
        Some(trimmed)
    }

    pub fn copy_selection(
        &mut self,
        clip_ids: &[ClipId],
        time: Option<(notwig_core::RangeMus, Vec<TrackId>)>,
    ) -> usize {
        if let Some((range, tracks)) = time {
            self.copy_time_range(range, &tracks)
        } else {
            self.copy_clips(clip_ids)
        }
    }

    pub fn cut_selection(
        &mut self,
        clip_ids: &[ClipId],
        time: Option<(notwig_core::RangeMus, Vec<TrackId>)>,
    ) -> usize {
        if let Some((range, tracks)) = time.clone() {
            let count = self.copy_time_range(range, &tracks);
            let _ = self.delete_time_range(range, &tracks);
            count
        } else {
            let count = self.copy_clips(clip_ids);
            for clip in clip_ids {
                let _ = self.apply(CommandPayload::DeleteClip { clip_id: *clip });
            }
            count
        }
    }

    pub fn paste_clips(&mut self, at: notwig_core::TimeMus) -> Result<PasteResult, String> {
        let entries = self.clipboard.clips.clone();
        if entries.is_empty() {
            return Ok(PasteResult::default());
        }
        let mut result = PasteResult::default();
        for entry in entries {
            if !self.doc.tracks.contains_key(&entry.track_id) {
                continue;
            }
            let start = (at.as_f64() + entry.offset.as_f64()).max(0.0);
            let start = notwig_core::TimeMus::new((start * 1_000_000.0).round() as i64, 1_000_000);
            let end = start + entry.clip.range.length();
            let range = notwig_core::RangeMus::new(start, end).map_err(|err| err.to_string())?;
            let payload = match &entry.clip.payload {
                ClipPayload::Midi(midi) => {
                    let notes = midi
                        .notes
                        .iter()
                        .cloned()
                        .map(|mut note| {
                            note.id = Id::<NoteMarker>::from_uuid(Uuid::new_v4());
                            note
                        })
                        .collect();
                    ClipPayload::Midi(notwig_core::MidiClipData {
                        length: midi.length,
                        content_start: midi.content_start,
                        notes,
                    })
                }
                ClipPayload::Audio(audio) => ClipPayload::Audio(audio.clone()),
            };
            let clip_id = Id::<ClipMarker>::from_uuid(Uuid::new_v4());
            let kind = entry.clip.kind;
            self.apply(CommandPayload::CreateClip {
                id: clip_id,
                track_id: entry.track_id,
                kind,
                range,
                payload,
            })?;
            result.clip_ids.push(clip_id);
            result.clip_tracks.push((clip_id, entry.track_id));
        }
        if self.clipboard.mode == ClipboardMode::Time {
            let range = self
                .clipboard
                .clips
                .iter()
                .map(|entry| entry.offset + entry.clip.range.length())
                .max();
            if let Some(length) = range
                && let Ok(pasted_range) = notwig_core::RangeMus::new(at, at + length)
            {
                let tracks = self
                    .clipboard
                    .time_tracks
                    .iter()
                    .copied()
                    .filter(|track| self.doc.tracks.contains_key(track))
                    .collect();
                result.time = Some((pasted_range, tracks));
            }
        }
        self.status = Some("Pasted selection.".into());
        Ok(result)
    }

    pub fn delete_time_range(
        &mut self,
        range: notwig_core::RangeMus,
        tracks: &[TrackId],
    ) -> Result<(), String> {
        let track_set: std::collections::BTreeSet<TrackId> = tracks.iter().copied().collect();
        let clips: Vec<notwig_core::ClipEntity> = self
            .doc
            .arranger
            .clips
            .values()
            .filter(|clip| track_set.contains(&clip.track_id) && clip.range.intersects(range))
            .cloned()
            .collect();
        for clip in clips {
            let clip_id = clip.id;
            if clip.range.start >= range.start && clip.range.end <= range.end {
                self.apply(CommandPayload::DeleteClip { clip_id })?;
            } else if clip.range.start < range.start && clip.range.end > range.end {
                let right_id = Id::<ClipMarker>::from_uuid(Uuid::new_v4());
                let mut new_note_ids = Vec::new();
                if let ClipPayload::Midi(midi) = &clip.payload {
                    let split_content = midi.content_start + (range.start - clip.range.start);
                    for note in &midi.notes {
                        if note.start < split_content && note.start + note.length > split_content {
                            new_note_ids.push(Id::<NoteMarker>::from_uuid(Uuid::new_v4()));
                        }
                    }
                }
                self.apply(CommandPayload::SplitClip {
                    clip_id,
                    at: range.start,
                    new_clip_id: right_id,
                    new_note_ids,
                })?;
                if range.end < clip.range.end {
                    self.edit_clip_range(
                        right_id,
                        notwig_core::RangeMus::new(range.end, clip.range.end)
                            .map_err(|err| err.to_string())?,
                    )?;
                } else {
                    self.apply(CommandPayload::DeleteClip { clip_id: right_id })?;
                }
            } else if clip.range.start < range.start {
                self.edit_clip_range(
                    clip_id,
                    notwig_core::RangeMus::new(clip.range.start, range.start)
                        .map_err(|err| err.to_string())?,
                )?;
            } else if clip.range.end > range.end {
                self.edit_clip_range(
                    clip_id,
                    notwig_core::RangeMus::new(range.end, clip.range.end)
                        .map_err(|err| err.to_string())?,
                )?;
            }
        }
        self.status = Some("Deleted time selection.".into());
        Ok(())
    }

    pub fn duplicate_time_range(
        &mut self,
        range: notwig_core::RangeMus,
        tracks: &[TrackId],
    ) -> Result<(), String> {
        let _ = self.copy_time_range(range, tracks);
        self.paste_clips(range.start + range.length()).map(|_| ())
    }

    fn edit_clip_range(
        &mut self,
        clip_id: ClipId,
        range: notwig_core::RangeMus,
    ) -> Result<(), String> {
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

    pub fn relink_asset(&mut self, asset_id: AssetId, path: &Path) -> Result<(), String> {
        let uri = path.to_string_lossy().into_owned();
        self.apply(CommandPayload::RelinkAsset { asset_id, uri })?;
        match notwig_project::decode_audio(path) {
            Ok(buffer) => {
                self.peaks.insert(asset_id, compute_peaks(&buffer, 2048));
                self.assets.insert(asset_id, Arc::new(buffer));
                self.rebuild_engine()?;
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
        let map = self.doc.tempo_map();
        let end_beat = self
            .doc
            .arranger
            .clips
            .values()
            .map(|clip| clip.range.end)
            .max()
            .unwrap_or_else(|| Rational::new(4, 1));
        let start = 0u64;
        let end = map.mus_to_samples(end_beat);
        let mut engine = self.build_engine()?;
        engine
            .apply(EngineMsg::SetTransport {
                playing: true,
                playhead_samples: start,
                loop_enabled: false,
                loop_start_samples: None,
                loop_end_samples: None,
                stop: false,
            })
            .map_err(|err| err.to_string())?;
        let output = engine
            .render_offline(start, end.max(start + 1))
            .map_err(|err| err.to_string())?;
        notwig_project::write_wav_f32(path, self.doc.meta.sample_rate.value(), 2, &output)
            .map_err(|err| err.to_string())?;
        Ok(output.len() / 2)
    }
}

fn trim_slice_window(
    map: notwig_core::TempoMap,
    slice: &notwig_core::sequencer::AudioSlice,
    from: notwig_core::TimeMus,
    to: notwig_core::TimeMus,
    new_origin: notwig_core::TimeMus,
) -> Option<notwig_core::sequencer::AudioSlice> {
    let duration = map.samples_to_mus(
        slice
            .source_end_sample
            .saturating_sub(slice.source_start_sample),
    );
    let start = slice.start;
    let end = start + duration;
    let window_start = from.max(start);
    let window_end = to.min(end);
    if window_end <= window_start {
        return None;
    }
    let base = i128::from(slice.source_start_sample);
    let max_end = i128::from(slice.source_end_sample);
    let source_start = base + i128::from(map.samples_delta(window_start - start));
    let source_end = base + i128::from(map.samples_delta(window_end - start));
    let source_start = source_start.clamp(0, max_end.saturating_sub(1));
    let source_end = source_end.clamp(source_start.saturating_add(1), max_end);
    if source_end <= source_start {
        return None;
    }
    let mut segment = slice.clone();
    segment.start = window_start - new_origin;
    segment.source_start_sample = u64::try_from(source_start).ok()?;
    segment.source_end_sample = u64::try_from(source_end).ok()?;
    Some(segment)
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

#[cfg(test)]
mod tests {
    use super::*;
    use notwig_core::ids::Asset as AssetMarker;
    use notwig_core::sequencer::NoteEntity;
    use notwig_core::{ClipKind, Norm01};

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("notwig-workspace-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn create_clip_save_and_render() {
        let dir = temp_dir();
        let mut workspace = Workspace::new_empty().unwrap();
        workspace.create_track(TrackKind::Instrument).unwrap();
        let track = workspace
            .doc
            .tracks
            .values()
            .find(|track| track.kind == TrackKind::Instrument)
            .unwrap()
            .id;
        workspace
            .create_midi_clip(
                track,
                notwig_core::RangeMus::new(Rational::zero(), Rational::new(2, 1)).unwrap(),
            )
            .unwrap();
        assert_eq!(workspace.doc.arranger.clips.len(), 1);
        let clip = *workspace.doc.arranger.clips.keys().next().unwrap();
        assert_eq!(workspace.doc.arranger.clips[&clip].kind, ClipKind::Midi);
        let bundle = dir.join("Test.notwig");
        workspace.save(Some(&bundle)).unwrap();
        let out = dir.join("mix.wav");
        let frames = workspace.render_to(&out).unwrap();
        assert!(frames > 0);
        assert!(out.exists());
        let reopened = Workspace::open(&bundle).unwrap();
        assert_eq!(reopened.doc.tracks.len(), workspace.doc.tracks.len());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn split_clip_generates_new_note_ids() {
        let mut workspace = Workspace::new_empty().unwrap();
        workspace.create_track(TrackKind::Instrument).unwrap();
        let track = workspace
            .doc
            .tracks
            .values()
            .find(|track| track.kind == TrackKind::Instrument)
            .unwrap()
            .id;
        workspace
            .create_midi_clip(
                track,
                notwig_core::RangeMus::new(Rational::zero(), Rational::new(4, 1)).unwrap(),
            )
            .unwrap();
        let clip = *workspace.doc.arranger.clips.keys().next().unwrap();
        let note_id = Id::<NoteMarker>::from_uuid(Uuid::new_v4());
        workspace
            .apply(CommandPayload::SetNote {
                clip_id: clip,
                note: NoteEntity {
                    id: note_id,
                    pitch: 60,
                    start: Rational::zero(),
                    length: Rational::new(2, 1),
                    velocity: Norm01::clamped(0.8),
                    release_velocity: None,
                    expression: std::collections::BTreeMap::default(),
                },
            })
            .unwrap();
        workspace.split_clip(clip, Rational::new(1, 1)).unwrap();
        assert_eq!(workspace.doc.arranger.clips.len(), 2);
        let notes = workspace
            .doc
            .arranger
            .clips
            .values()
            .filter_map(|clip| match &clip.payload {
                ClipPayload::Midi(midi) => Some(midi.notes.len()),
                ClipPayload::Audio(_) => None,
            })
            .sum::<usize>();
        assert_eq!(notes, 2);
    }

    #[test]
    fn stop_returns_to_play_start() {
        let mut workspace = Workspace::new_empty().unwrap();
        workspace.set_playhead(Rational::new(2, 1)).unwrap();
        workspace.play().unwrap();
        assert_eq!(workspace.play_start, Some(Rational::new(2, 1)));
        workspace.set_playhead(Rational::new(5, 1)).unwrap();
        workspace.stop().unwrap();
        assert!(!workspace.doc.transport.playing);
        assert_eq!(workspace.doc.transport.playhead, Rational::new(2, 1));
    }

    #[test]
    fn import_audio_at_places_clip_on_track() {
        let dir = temp_dir();
        let mut workspace = Workspace::new_empty().unwrap();
        workspace.create_track(TrackKind::Audio).unwrap();
        let track = workspace
            .doc
            .tracks
            .values()
            .find(|track| track.kind == TrackKind::Audio)
            .unwrap()
            .id;
        workspace.save(Some(&dir.join("Import.notwig"))).unwrap();
        let source = dir.join("tone.wav");
        let samples: Vec<f32> = (0..4800).map(|i| (i as f32 * 0.02).sin()).collect();
        notwig_project::write_wav_f32(&source, 48_000, 1, &samples).unwrap();
        workspace
            .import_audio_at(&source, track, Rational::new(2, 1))
            .unwrap();
        assert_eq!(workspace.doc.assets.assets.len(), 1);
        let clip = workspace.doc.arranger.clips.values().next().unwrap();
        assert_eq!(clip.range.start, Rational::new(2, 1));
        assert_eq!(workspace.assets.len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn device_parameters_affect_offline_render() {
        let dir = temp_dir();
        let mut workspace = Workspace::new_empty().unwrap();
        workspace.create_track(TrackKind::Audio).unwrap();
        let track = workspace
            .doc
            .tracks
            .values()
            .find(|track| track.kind == TrackKind::Audio)
            .unwrap()
            .clone();
        workspace.save(Some(&dir.join("Devices.notwig"))).unwrap();
        let source = dir.join("tone.wav");
        let samples: Vec<f32> = (0..24_000).map(|_| 0.5).collect();
        notwig_project::write_wav_f32(&source, 48_000, 1, &samples).unwrap();
        workspace
            .import_audio_at(&source, track.id, Rational::zero())
            .unwrap();
        workspace
            .insert_stock(
                track.main_chain,
                notwig_dsp::stock::TOOL_TYPE_ID,
                DeviceKind::AudioFx,
                None,
            )
            .unwrap();
        let device = *workspace.doc.devices.keys().next().unwrap();
        workspace
            .apply(CommandPayload::SetParam {
                device_id: device,
                param_id: notwig_dsp::stock::p_output_gain(),
                value: notwig_core::ParamValue::F64(-60.0),
                ramp: None,
            })
            .unwrap();
        let out = dir.join("device.wav");
        workspace.render_to(&out).unwrap();
        let rendered = notwig_project::decode_audio(&out).unwrap();
        let peak = rendered
            .samples
            .iter()
            .fold(0.0_f32, |acc, value| acc.max(value.abs()));
        assert!(peak < 0.01, "device gain should apply, peak={peak}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn imports_and_renders_mp3() {
        let dir = temp_dir();
        let mut workspace = Workspace::new_empty().unwrap();
        workspace.create_track(TrackKind::Audio).unwrap();
        let track = workspace
            .doc
            .tracks
            .values()
            .find(|track| track.kind == TrackKind::Audio)
            .unwrap()
            .id;
        workspace.save(Some(&dir.join("Mp3.notwig"))).unwrap();
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../notwig-project/tests/fixtures/tone.mp3");
        workspace
            .import_audio_at(&source, track, Rational::zero())
            .unwrap();
        assert_eq!(workspace.doc.assets.assets.len(), 1);
        let out = dir.join("mp3.wav");
        assert!(workspace.render_to(&out).unwrap() > 0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn copy_paste_and_time_range_edits_work() {
        let mut workspace = Workspace::new_empty().unwrap();
        workspace.create_track(TrackKind::Instrument).unwrap();
        let track = workspace
            .doc
            .tracks
            .values()
            .find(|track| track.kind == TrackKind::Instrument)
            .unwrap()
            .id;
        workspace
            .create_midi_clip(
                track,
                notwig_core::RangeMus::new(Rational::zero(), Rational::new(4, 1)).unwrap(),
            )
            .unwrap();
        let clip = *workspace.doc.arranger.clips.keys().next().unwrap();
        workspace.copy_clips(&[clip]);
        let result = workspace.paste_clips(Rational::new(4, 1)).unwrap();
        assert_eq!(workspace.doc.arranger.clips.len(), 2);
        assert_eq!(result.clip_ids.len(), 1);
        assert!(result.time.is_none());
    }

    #[test]
    fn node_duplicate_places_a_copy_after_the_source_clip() {
        let mut workspace = Workspace::new_empty().unwrap();
        workspace.create_track(TrackKind::Instrument).unwrap();
        let track = workspace
            .doc
            .tracks
            .values()
            .find(|track| track.kind == TrackKind::Instrument)
            .unwrap()
            .id;
        workspace
            .create_midi_clip(
                track,
                notwig_core::RangeMus::new(Rational::zero(), Rational::new(2, 1)).unwrap(),
            )
            .unwrap();
        let clip = *workspace.doc.arranger.clips.keys().next().unwrap();
        workspace.duplicate_clip(clip).unwrap();
        let starts: Vec<_> = workspace
            .doc
            .arranger
            .clips
            .values()
            .map(|clip| clip.range.start)
            .collect();
        assert!(starts.contains(&Rational::zero()));
        assert!(starts.contains(&Rational::new(2, 1)));
    }

    #[test]
    fn delete_time_range_splits_multiple_audio_slices() {
        let mut workspace = Workspace::new_empty().unwrap();
        workspace.create_track(TrackKind::Audio).unwrap();
        let track = workspace
            .doc
            .tracks
            .values()
            .find(|track| track.kind == TrackKind::Audio)
            .unwrap()
            .id;
        let asset = Id::<AssetMarker>::from_uuid(Uuid::from_u128(600));
        workspace
            .apply(CommandPayload::RegisterAsset {
                asset_id: asset,
                entry: notwig_core::AssetEntry {
                    id: asset,
                    kind: notwig_core::AssetKind::Audio,
                    uri: "assets/test.wav".into(),
                    hash: None,
                    format: "wav".into(),
                    sample_rate: Some(48_000),
                    channels: Some(2),
                    duration_samples: Some(96_000),
                    original_sample_rate: Some(48_000),
                    original_channels: Some(2),
                },
            })
            .unwrap();
        let clip = Id::<ClipMarker>::from_uuid(Uuid::from_u128(700));
        workspace
            .apply(CommandPayload::CreateClip {
                id: clip,
                track_id: track,
                kind: ClipKind::Audio,
                range: notwig_core::RangeMus::new(Rational::zero(), Rational::new(4, 1)).unwrap(),
                payload: ClipPayload::Audio(notwig_core::AudioClipData {
                    asset_id: asset,
                    source_start_sample: 0,
                    source_end_sample: 96_000,
                    gain_db: Db(0.0),
                    fade_in_samples: 0,
                    fade_out_samples: 0,
                    fade_shape: notwig_core::sequencer::FadeShape::Linear,
                    slices: vec![
                        notwig_core::sequencer::AudioSlice {
                            id: notwig_core::SliceId::from_uuid(Uuid::from_u128(1)),
                            start: Rational::zero(),
                            source_start_sample: 0,
                            source_end_sample: 48_000,
                            gain_db: Db(0.0),
                            asset_id: None,
                        },
                        notwig_core::sequencer::AudioSlice {
                            id: notwig_core::SliceId::from_uuid(Uuid::from_u128(2)),
                            start: Rational::new(2, 1),
                            source_start_sample: 48_000,
                            source_end_sample: 96_000,
                            gain_db: Db(0.0),
                            asset_id: None,
                        },
                    ],
                    slices_enabled: true,
                }),
            })
            .unwrap();

        workspace
            .delete_time_range(
                notwig_core::RangeMus::new(Rational::new(1, 1), Rational::new(2, 1)).unwrap(),
                &[track],
            )
            .unwrap();

        let clips: Vec<_> = workspace
            .doc
            .arranger
            .clips
            .values()
            .filter(|clip| clip.track_id == track)
            .collect();
        assert_eq!(clips.len(), 2);
        let left = clips
            .iter()
            .find(|clip| clip.range.start == Rational::zero())
            .unwrap();
        let right = clips
            .iter()
            .find(|clip| clip.range.start == Rational::new(2, 1))
            .unwrap();
        let ClipPayload::Audio(left_audio) = &left.payload else {
            panic!("left audio");
        };
        let ClipPayload::Audio(right_audio) = &right.payload else {
            panic!("right audio");
        };
        assert_eq!(left_audio.slices.len(), 1);
        assert_eq!(left_audio.slices[0].source_end_sample, 24_000);
        assert_eq!(right_audio.slices.len(), 1);
        assert_eq!(right_audio.slices[0].start, Rational::zero());
        assert_eq!(right_audio.slices[0].source_start_sample, 48_000);
    }

    #[test]
    fn adding_an_audio_slice_extends_the_clip_and_references_its_asset() {
        let mut workspace = Workspace::new_empty().unwrap();
        workspace.create_track(TrackKind::Audio).unwrap();
        let track = workspace
            .doc
            .tracks
            .values()
            .find(|track| track.kind == TrackKind::Audio)
            .unwrap()
            .id;
        let register = |workspace: &mut Workspace, asset: Id<AssetMarker>, duration: u64| {
            workspace
                .apply(CommandPayload::RegisterAsset {
                    asset_id: asset,
                    entry: notwig_core::AssetEntry {
                        id: asset,
                        kind: notwig_core::AssetKind::Audio,
                        uri: format!("assets/{asset}.wav"),
                        hash: None,
                        format: "wav".into(),
                        sample_rate: Some(48_000),
                        channels: Some(2),
                        duration_samples: Some(duration),
                        original_sample_rate: Some(48_000),
                        original_channels: Some(2),
                    },
                })
                .unwrap();
        };
        let base = Id::<AssetMarker>::from_uuid(Uuid::from_u128(700));
        let dropped = Id::<AssetMarker>::from_uuid(Uuid::from_u128(701));
        register(&mut workspace, base, 48_000);
        register(&mut workspace, dropped, 48_000);
        let clip = Id::<ClipMarker>::from_uuid(Uuid::from_u128(702));
        workspace
            .apply(CommandPayload::CreateClip {
                id: clip,
                track_id: track,
                kind: ClipKind::Audio,
                range: notwig_core::RangeMus::new(Rational::zero(), Rational::new(2, 1)).unwrap(),
                payload: ClipPayload::Audio(notwig_core::AudioClipData {
                    asset_id: base,
                    source_start_sample: 0,
                    source_end_sample: 48_000,
                    gain_db: Db(0.0),
                    fade_in_samples: 0,
                    fade_out_samples: 0,
                    fade_shape: notwig_core::sequencer::FadeShape::Linear,
                    slices: Vec::new(),
                    slices_enabled: false,
                }),
            })
            .unwrap();

        workspace
            .add_audio_slice(clip, dropped, Rational::new(1, 1))
            .unwrap();
        let clip = &workspace.doc.arranger.clips[&clip];
        assert_eq!(clip.range.end, Rational::new(3, 1));
        let ClipPayload::Audio(audio) = &clip.payload else {
            panic!("audio clip");
        };
        assert_eq!(audio.slices.len(), 2);
        let added = audio
            .slices
            .iter()
            .find(|slice| slice.asset_id == Some(dropped))
            .expect("dropped slice");
        assert_eq!(added.start, Rational::new(1, 1));
        assert_eq!(added.source_end_sample, 48_000);
    }

    #[test]
    fn duplicate_clears_the_destination_range_before_inserting() {
        let mut workspace = Workspace::new_empty().unwrap();
        workspace.create_track(TrackKind::Instrument).unwrap();
        let track = workspace
            .doc
            .tracks
            .values()
            .find(|track| track.kind == TrackKind::Instrument)
            .unwrap()
            .id;
        workspace
            .create_midi_clip(
                track,
                notwig_core::RangeMus::new(Rational::zero(), Rational::new(2, 1)).unwrap(),
            )
            .unwrap();
        workspace
            .create_midi_clip(
                track,
                notwig_core::RangeMus::new(Rational::new(2, 1), Rational::new(6, 1)).unwrap(),
            )
            .unwrap();
        let source = workspace
            .doc
            .arranger
            .clips
            .values()
            .find(|clip| clip.range.start == Rational::zero())
            .unwrap()
            .id;
        let new_id = Id::<ClipMarker>::from_uuid(Uuid::from_u128(1001));
        workspace
            .duplicate_clip_to(
                source,
                new_id,
                Some(notwig_core::RangeMus::new(Rational::new(3, 1), Rational::new(5, 1)).unwrap()),
            )
            .unwrap();

        let mut ranges: Vec<_> = workspace
            .doc
            .arranger
            .clips
            .values()
            .filter(|clip| clip.track_id == track)
            .map(|clip| (clip.range.start, clip.range.end))
            .collect();
        ranges.sort();
        assert_eq!(
            ranges,
            vec![
                (Rational::zero(), Rational::new(2, 1)),
                (Rational::new(2, 1), Rational::new(3, 1)),
                (Rational::new(3, 1), Rational::new(5, 1)),
                (Rational::new(5, 1), Rational::new(6, 1)),
            ]
        );
    }

    #[test]
    fn copying_a_time_range_trims_audio_slice_windows() {
        let mut workspace = Workspace::new_empty().unwrap();
        workspace.create_track(TrackKind::Audio).unwrap();
        let track = workspace
            .doc
            .tracks
            .values()
            .find(|track| track.kind == TrackKind::Audio)
            .unwrap()
            .id;
        let asset = Id::<AssetMarker>::from_uuid(Uuid::from_u128(800));
        workspace
            .apply(CommandPayload::RegisterAsset {
                asset_id: asset,
                entry: notwig_core::AssetEntry {
                    id: asset,
                    kind: notwig_core::AssetKind::Audio,
                    uri: "assets/copy.wav".into(),
                    hash: None,
                    format: "wav".into(),
                    sample_rate: Some(48_000),
                    channels: Some(2),
                    duration_samples: Some(96_000),
                    original_sample_rate: Some(48_000),
                    original_channels: Some(2),
                },
            })
            .unwrap();
        workspace
            .apply(CommandPayload::CreateClip {
                id: Id::<ClipMarker>::from_uuid(Uuid::from_u128(801)),
                track_id: track,
                kind: ClipKind::Audio,
                range: notwig_core::RangeMus::new(Rational::zero(), Rational::new(4, 1)).unwrap(),
                payload: ClipPayload::Audio(notwig_core::AudioClipData {
                    asset_id: asset,
                    source_start_sample: 0,
                    source_end_sample: 96_000,
                    gain_db: Db(0.0),
                    fade_in_samples: 0,
                    fade_out_samples: 0,
                    fade_shape: notwig_core::sequencer::FadeShape::Linear,
                    slices: vec![
                        notwig_core::sequencer::AudioSlice {
                            id: notwig_core::SliceId::from_uuid(Uuid::from_u128(1)),
                            start: Rational::zero(),
                            source_start_sample: 0,
                            source_end_sample: 48_000,
                            gain_db: Db(0.0),
                            asset_id: None,
                        },
                        notwig_core::sequencer::AudioSlice {
                            id: notwig_core::SliceId::from_uuid(Uuid::from_u128(2)),
                            start: Rational::new(2, 1),
                            source_start_sample: 48_000,
                            source_end_sample: 96_000,
                            gain_db: Db(0.0),
                            asset_id: None,
                        },
                    ],
                    slices_enabled: true,
                }),
            })
            .unwrap();
        workspace.copy_time_range(
            notwig_core::RangeMus::new(Rational::new(1, 1), Rational::new(3, 1)).unwrap(),
            &[track],
        );
        let result = workspace.paste_clips(Rational::new(4, 1)).unwrap();
        let pasted = &workspace.doc.arranger.clips[&result.clip_ids[0]];
        let ClipPayload::Audio(audio) = &pasted.payload else {
            panic!("pasted audio");
        };
        assert_eq!(audio.slices.len(), 2);
        assert_eq!(audio.slices[0].start, Rational::zero());
        assert_eq!(audio.slices[0].source_start_sample, 24_000);
        assert_eq!(audio.slices[0].source_end_sample, 48_000);
        assert_eq!(audio.slices[1].start, Rational::new(1, 1));
        assert_eq!(audio.slices[1].source_start_sample, 48_000);
        assert_eq!(audio.slices[1].source_end_sample, 72_000);
    }

    #[test]
    fn paste_clears_the_destination_range_before_inserting() {
        let mut workspace = Workspace::new_empty().unwrap();
        workspace.create_track(TrackKind::Instrument).unwrap();
        let track = workspace
            .doc
            .tracks
            .values()
            .find(|track| track.kind == TrackKind::Instrument)
            .unwrap()
            .id;
        workspace
            .create_midi_clip(
                track,
                notwig_core::RangeMus::new(Rational::zero(), Rational::new(2, 1)).unwrap(),
            )
            .unwrap();
        let source = *workspace.doc.arranger.clips.keys().next().unwrap();
        workspace.copy_clips(&[source]);
        workspace.paste_clips(Rational::new(1, 1)).unwrap();
        let mut ranges: Vec<_> = workspace
            .doc
            .arranger
            .clips
            .values()
            .filter(|clip| clip.track_id == track)
            .map(|clip| (clip.range.start, clip.range.end))
            .collect();
        ranges.sort();
        assert_eq!(
            ranges,
            vec![
                (Rational::zero(), Rational::new(1, 1)),
                (Rational::new(1, 1), Rational::new(3, 1)),
            ]
        );
    }

    #[test]
    fn duplicate_clip_to_uses_the_requested_id_and_range() {
        let mut workspace = Workspace::new_empty().unwrap();
        workspace.create_track(TrackKind::Instrument).unwrap();
        let track = workspace
            .doc
            .tracks
            .values()
            .find(|track| track.kind == TrackKind::Instrument)
            .unwrap()
            .id;
        workspace
            .create_midi_clip(
                track,
                notwig_core::RangeMus::new(Rational::zero(), Rational::new(2, 1)).unwrap(),
            )
            .unwrap();
        let clip = *workspace.doc.arranger.clips.keys().next().unwrap();
        let new_id = Id::<ClipMarker>::from_uuid(Uuid::from_u128(999));
        let range = notwig_core::RangeMus::new(Rational::new(5, 1), Rational::new(7, 1)).unwrap();
        workspace
            .duplicate_clip_to(clip, new_id, Some(range))
            .unwrap();
        assert_eq!(workspace.doc.arranger.clips[&new_id].range, range);
    }

    #[test]
    fn duplicate_and_delete_time_range_work() {
        let mut workspace = Workspace::new_empty().unwrap();
        workspace.create_track(TrackKind::Instrument).unwrap();
        let track = workspace
            .doc
            .tracks
            .values()
            .find(|track| track.kind == TrackKind::Instrument)
            .unwrap()
            .id;
        workspace
            .create_midi_clip(
                track,
                notwig_core::RangeMus::new(Rational::zero(), Rational::new(4, 1)).unwrap(),
            )
            .unwrap();
        workspace
            .duplicate_time_range(
                notwig_core::RangeMus::new(Rational::zero(), Rational::new(4, 1)).unwrap(),
                &[track],
            )
            .unwrap();
        assert_eq!(workspace.doc.arranger.clips.len(), 2);

        workspace
            .delete_time_range(
                notwig_core::RangeMus::new(Rational::new(1, 1), Rational::new(3, 1)).unwrap(),
                &[track],
            )
            .unwrap();
        let clips: Vec<_> = workspace.doc.arranger.clips.values().collect();
        assert!(clips.iter().any(|clip| {
            clip.range.start == Rational::zero() && clip.range.end == Rational::new(1, 1)
        }));
        assert!(clips.iter().any(|clip| {
            clip.range.start == Rational::new(3, 1) && clip.range.end == Rational::new(4, 1)
        }));
    }

    #[test]
    fn copy_bar_selection_then_paste_at_new_time() {
        let mut workspace = Workspace::new_empty().unwrap();
        workspace.create_track(TrackKind::Instrument).unwrap();
        let track = workspace
            .doc
            .tracks
            .values()
            .find(|track| track.kind == TrackKind::Instrument)
            .unwrap()
            .id;
        workspace
            .create_midi_clip(
                track,
                notwig_core::RangeMus::new(Rational::zero(), Rational::new(4, 1)).unwrap(),
            )
            .unwrap();
        workspace.copy_selection(
            &[],
            Some((
                notwig_core::RangeMus::new(Rational::zero(), Rational::new(2, 1)).unwrap(),
                vec![track],
            )),
        );
        let result = workspace.paste_clips(Rational::new(8, 1)).unwrap();
        assert!(
            workspace
                .doc
                .arranger
                .clips
                .values()
                .any(|clip| clip.range.start == Rational::new(8, 1))
        );
        let (range, tracks) = result.time.expect("bar selection should be selected");
        assert_eq!(range.start, Rational::new(8, 1));
        assert_eq!(range.end, Rational::new(10, 1));
        assert_eq!(tracks, vec![track]);
    }

    #[test]
    fn grouping_tracks_moves_them_under_a_group_bus() {
        let mut workspace = Workspace::new_empty().unwrap();
        workspace.create_track(TrackKind::Audio).unwrap();
        workspace.create_track(TrackKind::Audio).unwrap();
        let tracks: Vec<_> = workspace
            .doc
            .tracks
            .values()
            .filter(|track| track.kind == TrackKind::Audio)
            .map(|track| track.id)
            .collect();
        let group = workspace.group_tracks(&tracks).unwrap();
        let group_track = workspace.doc.tracks.get(&group).unwrap();
        assert_eq!(group_track.kind, TrackKind::Group);
        for track in &tracks {
            assert_eq!(workspace.doc.tracks[track].parent, Some(group));
        }
        let children = workspace
            .doc
            .track_order
            .children
            .get(&group)
            .expect("group children");
        assert_eq!(children.len(), tracks.len());
        for track in &tracks {
            assert!(children.contains(track));
        }
    }

    #[test]
    fn track_color_and_comment_round_trip() {
        let mut workspace = Workspace::new_empty().unwrap();
        workspace.create_track(TrackKind::Audio).unwrap();
        let track = workspace
            .doc
            .tracks
            .values()
            .find(|track| track.kind == TrackKind::Audio)
            .unwrap()
            .id;
        let color = notwig_core::Color {
            r: 64,
            g: 128,
            b: 192,
            a: 255,
        };
        workspace
            .apply(CommandPayload::SetTrackColor {
                id: track,
                color: Some(color),
            })
            .unwrap();
        workspace
            .apply(CommandPayload::SetTrackComment {
                id: track,
                comment: "Lead bus".into(),
            })
            .unwrap();
        assert_eq!(workspace.doc.tracks[&track].color, Some(color));
        assert_eq!(workspace.doc.tracks[&track].comment, "Lead bus");
    }

    #[test]
    fn duplicating_a_track_copies_clips_and_devices() {
        let mut workspace = Workspace::new_empty().unwrap();
        workspace.create_track(TrackKind::Instrument).unwrap();
        let track = workspace
            .doc
            .tracks
            .values()
            .find(|track| track.kind == TrackKind::Instrument)
            .unwrap()
            .id;
        workspace
            .create_midi_clip(
                track,
                notwig_core::RangeMus::new(Rational::zero(), Rational::new(4, 1)).unwrap(),
            )
            .unwrap();
        workspace
            .insert_stock(
                workspace.doc.tracks[&track].main_chain,
                notwig_dsp::stock::TOOL_TYPE_ID,
                DeviceKind::AudioFx,
                None,
            )
            .unwrap();
        let copy = workspace.duplicate_track(track).unwrap();
        assert_eq!(workspace.doc.tracks[&copy].parent, None);
        assert_eq!(
            workspace.doc.arranger.lanes[&copy].clips.len(),
            workspace.doc.arranger.lanes[&track].clips.len()
        );
        assert_eq!(
            workspace.doc.chains[&workspace.doc.tracks[&copy].main_chain]
                .devices
                .len(),
            1
        );
    }

    #[test]
    fn grouping_a_group_keeps_the_track_forest_valid() {
        let mut workspace = Workspace::new_empty().unwrap();
        workspace.create_track(TrackKind::Audio).unwrap();
        workspace.create_track(TrackKind::Audio).unwrap();
        let tracks: Vec<_> = workspace
            .doc
            .tracks
            .values()
            .filter(|track| track.kind == TrackKind::Audio)
            .map(|track| track.id)
            .collect();
        let inner = workspace.group_tracks(&tracks).unwrap();
        let outer = workspace.group_tracks(&[inner]).unwrap();
        assert_eq!(workspace.doc.tracks[&inner].parent, Some(outer));
        workspace.doc.validate().unwrap();
    }

    #[test]
    fn undo_redo_restores_document_snapshots() {
        let mut workspace = Workspace::new_empty().unwrap();
        workspace.create_track(TrackKind::Instrument).unwrap();
        let track = workspace
            .doc
            .tracks
            .values()
            .find(|track| track.kind == TrackKind::Instrument)
            .unwrap()
            .id;
        workspace
            .create_midi_clip(
                track,
                notwig_core::RangeMus::new(Rational::zero(), Rational::new(4, 1)).unwrap(),
            )
            .unwrap();
        assert_eq!(workspace.doc.arranger.clips.len(), 1);
        workspace.undo().unwrap();
        assert_eq!(workspace.doc.arranger.clips.len(), 0);
        workspace.redo().unwrap();
        assert_eq!(workspace.doc.arranger.clips.len(), 1);
    }

    #[test]
    fn inserting_stock_device_rebuilds_engine() {
        let mut workspace = Workspace::new_empty().unwrap();
        workspace.create_track(TrackKind::Audio).unwrap();
        let track = workspace
            .doc
            .tracks
            .values()
            .find(|track| track.kind == TrackKind::Audio)
            .unwrap()
            .clone();
        workspace
            .insert_stock(
                track.main_chain,
                notwig_dsp::stock::TOOL_TYPE_ID,
                DeviceKind::AudioFx,
                None,
            )
            .unwrap();
        assert_eq!(workspace.doc.devices.len(), 1);
        let engine = workspace.engine.lock().unwrap();
        assert!(engine.playhead_samples() < 1_000_000);
    }
}
