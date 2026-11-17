use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use notwig_core::ids::{
    Clip as ClipMarker, Device as DeviceMarker, DeviceChain as DeviceChainMarker,
    Note as NoteMarker, Track as TrackMarker,
};
use notwig_core::protocol::{Command, CommandPayload, EngineMsg, PlanHandle, PlaybackSchedule};
use notwig_core::sequencer::MidiClipData;
use notwig_core::{
    AssetId, AudioAssetBuffer, ClipId, ClipPayload, Db, DeviceChainId, DeviceKind, Id, Meta,
    ProjectDoc, Rational, SampleRate, SchemaVersion, TrackId, TrackKind, apply,
};
use notwig_dsp::EngineCaps;
use notwig_engine::{Engine, EngineConfig, PluginDeviceFactory};
use notwig_plugins::PluginHost;
use notwig_ui::PluginEntry;
use uuid::Uuid;

pub struct Workspace {
    pub doc: ProjectDoc,
    pub bundle: Option<PathBuf>,
    pub dirty: bool,
    pub assets: BTreeMap<AssetId, Arc<AudioAssetBuffer>>,
    pub peaks: BTreeMap<AssetId, Vec<f32>>,
    pub engine: Arc<Mutex<Engine>>,
    pub plugins: Vec<PluginEntry>,
    pub status: Option<String>,
    pub error: Option<String>,
    pub last_autosave: std::time::Instant,
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
        let host = new_plugin_host();
        let plugins = plugin_entries(&host);
        let engine = Arc::new(Mutex::new(Engine::new(EngineConfig::default())));
        let mut workspace = Self {
            doc,
            bundle,
            dirty,
            assets: BTreeMap::new(),
            peaks: BTreeMap::new(),
            engine,
            plugins,
            status: None,
            error: None,
            last_autosave: std::time::Instant::now(),
        };
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
        let plan = notwig_dsp::compile(&self.doc, &notwig_dsp::stock_registry(), caps)
            .map_err(|err| err.to_string())?;
        let config = EngineConfig::new(self.doc.meta.sample_rate.value(), 512);
        let factory = Arc::new(PluginDeviceFactory::new(new_plugin_host()));
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
        let structural = matches!(
            payload,
            CommandPayload::CreateTrack { .. }
                | CommandPayload::DeleteTrack { .. }
                | CommandPayload::MoveTrack { .. }
                | CommandPayload::SetTrackRoute { .. }
                | CommandPayload::SetSend { .. }
                | CommandPayload::RemoveSend { .. }
                | CommandPayload::InsertDevice { .. }
                | CommandPayload::RemoveDevice { .. }
                | CommandPayload::MoveDevice { .. }
                | CommandPayload::SetDeviceBypass { .. }
                | CommandPayload::SetDeviceState { .. }
                | CommandPayload::RegisterAsset { .. }
                | CommandPayload::RelinkAsset { .. }
        );
        match apply(&self.doc, &Command::new(payload)) {
            Ok(result) => {
                self.doc = result.doc;
                self.dirty = true;
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
        self.apply(CommandPayload::Stop)
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
        let track = Id::<TrackMarker>::from_uuid(Uuid::new_v4());
        let chain = Id::<DeviceChainMarker>::from_uuid(Uuid::new_v4());
        let name = format!("{} {}", kind.name(), self.doc.tracks.len());
        self.apply(CommandPayload::CreateTrack {
            id: track,
            chain_id: chain,
            kind,
            name,
            parent: None,
            index: None,
            color: None,
        })
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

    pub fn duplicate_clip(&mut self, clip_id: ClipId) -> Result<(), String> {
        let Some(clip) = self.doc.arranger.clips.get(&clip_id) else {
            return Err("clip not found".into());
        };
        let new_clip = Id::<ClipMarker>::from_uuid(Uuid::new_v4());
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
            range: None,
        })
    }

    pub fn insert_stock(
        &mut self,
        chain_id: DeviceChainId,
        type_id: &str,
        kind: DeviceKind,
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
            index: None,
            child_chain_ids: child_chains,
            name: None,
        })
    }

    pub fn insert_plugin(
        &mut self,
        chain_id: DeviceChainId,
        type_id: &str,
        kind: DeviceKind,
    ) -> Result<(), String> {
        let id = Id::<DeviceMarker>::from_uuid(Uuid::new_v4());
        self.apply(CommandPayload::InsertDevice {
            id,
            type_id: type_id.into(),
            version: None,
            kind,
            target_chain: chain_id,
            index: None,
            child_chain_ids: Vec::new(),
            name: None,
        })
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
            }),
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
    let mut host = PluginHost::new();
    let mut paths = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        paths.push(home.join("Library/Audio/Plug-Ins/VST3"));
        paths.push(home.join("Library/Audio/Plug-Ins/Components"));
    }
    paths.push(PathBuf::from("/Library/Audio/Plug-Ins/VST3"));
    paths.push(PathBuf::from("/Library/Audio/Plug-Ins/Components"));
    host.scan(&paths);
    host
}

fn plugin_entries(host: &PluginHost) -> Vec<PluginEntry> {
    host.cache()
        .descriptors()
        .into_iter()
        .map(|descriptor| PluginEntry {
            type_id: descriptor.type_id,
            name: descriptor.name,
            vendor: descriptor.vendor,
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
            )
            .unwrap();
        assert_eq!(workspace.doc.devices.len(), 1);
        let engine = workspace.engine.lock().unwrap();
        assert!(engine.playhead_samples() < 1_000_000);
    }
}
