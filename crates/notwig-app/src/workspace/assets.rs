#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
use std::sync::Arc;

use notwig_core::protocol::CommandPayload;
use notwig_core::{AssetId, AudioAssetBuffer, Id};
#[cfg(not(target_arch = "wasm32"))]
use notwig_core::{ClipId, ClipPayload, Db, TrackId};
use uuid::Uuid;

use super::Workspace;

#[cfg(not(target_arch = "wasm32"))]
impl Workspace {
    pub(super) fn load_assets(&mut self) {
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

    pub(super) fn prepare_asset(&mut self) {
        let assets = Arc::new(self.assets.clone());
        self.engine_control.data_mut().set_assets(assets);
        self.engine_control.flush();
    }

    pub fn import_audio(&mut self, source: &Path) -> Result<AssetId, String> {
        let asset_id = Id::<notwig_core::ids::Asset>::from_uuid(Uuid::new_v4());
        let pending = self.bundle.is_none();
        let bundle = if let Some(bundle) = self.bundle.clone() {
            bundle
        } else {
            let dir = std::env::temp_dir().join(format!("notwig-import-{asset_id}"));
            std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
            self.pending_import_dirs.insert(asset_id, dir.clone());
            dir
        };
        let imported = match notwig_project::import_asset(
            source,
            &bundle,
            asset_id,
            self.doc.meta.sample_rate.value(),
        ) {
            Ok(imported) => imported,
            Err(err) => {
                if pending {
                    self.pending_import_dirs.remove(&asset_id);
                    let _ = std::fs::remove_dir_all(&bundle);
                }
                return Err(err.to_string());
            }
        };
        self.peaks
            .insert(asset_id, compute_peaks(&imported.buffer, 2048));
        let buffer = Arc::new(imported.buffer);
        self.assets.insert(asset_id, buffer);
        self.apply(CommandPayload::RegisterAsset {
            asset_id,
            entry: imported.entry,
        })?;
        self.prepare_asset();
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
                silent_prefix_samples: 0,
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
            silent_prefix_samples: 0,
            gain_db: Db(0.0),
            asset_id: Some(asset_id),
        });
        self.apply(CommandPayload::SetClipSlices { clip_id, slices })?;
        self.status = Some("Added audio event.".into());
        Ok(())
    }

    pub fn relink_asset(&mut self, asset_id: AssetId, path: &Path) -> Result<(), String> {
        let uri = path.to_string_lossy().into_owned();
        self.apply(CommandPayload::RelinkAsset { asset_id, uri })?;
        match notwig_project::decode_audio(path) {
            Ok(buffer) => {
                self.peaks.insert(asset_id, compute_peaks(&buffer, 2048));
                let buffer = Arc::new(buffer);
                self.assets.insert(asset_id, Arc::clone(&buffer));
                self.prepare_asset();
                self.status = Some(format!("Relinked asset to {}", path.display()));
                Ok(())
            }
            Err(err) => {
                self.error = Some(err.to_string());
                Err(err.to_string())
            }
        }
    }
}

pub(super) fn compute_peaks(buffer: &AudioAssetBuffer, buckets: usize) -> Vec<f32> {
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

#[cfg(not(target_arch = "wasm32"))]
impl Workspace {
    pub(super) fn persist_asset_file(
        &mut self,
        asset_id: AssetId,
        buffer: &AudioAssetBuffer,
    ) -> Result<(), String> {
        let Some(dir) = self
            .bundle
            .clone()
            .or_else(|| self.pending_import_dirs.get(&asset_id).cloned())
        else {
            return Ok(());
        };
        let assets_dir = dir.join("assets");
        std::fs::create_dir_all(&assets_dir).map_err(|err| err.to_string())?;
        let file_name = format!("{asset_id}-{}.wav", buffer.sample_rate);
        notwig_project::write_wav_f32(
            &assets_dir.join(&file_name),
            buffer.sample_rate,
            buffer.channels,
            &buffer.samples,
        )
        .map_err(|err| err.to_string())?;
        if let Some(entry) = self.doc.assets.assets.get_mut(&asset_id) {
            entry.uri = format!("assets/{file_name}");
            entry.hash = None;
        }
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
enum AudioImportPlacement {
    Clip {
        track_id: TrackId,
        start: notwig_core::TimeMus,
    },
    Slice {
        clip_id: ClipId,
        at: notwig_core::TimeMus,
    },
}

#[cfg(not(target_arch = "wasm32"))]
impl Workspace {
    pub fn stage_audio_clip_import(
        &mut self,
        source: &Path,
        track_id: TrackId,
        start: notwig_core::TimeMus,
    ) -> Result<(AssetId, std::path::PathBuf), String> {
        self.stage_audio_import(source, AudioImportPlacement::Clip { track_id, start })
    }

    pub fn stage_audio_slice_import(
        &mut self,
        source: &Path,
        clip_id: ClipId,
        at: notwig_core::TimeMus,
    ) -> Result<(AssetId, std::path::PathBuf), String> {
        self.stage_audio_import(source, AudioImportPlacement::Slice { clip_id, at })
    }

    fn stage_audio_import(
        &mut self,
        source: &Path,
        placement: AudioImportPlacement,
    ) -> Result<(AssetId, std::path::PathBuf), String> {
        let probe = notwig_project::probe_audio(source).map_err(|err| err.to_string())?;
        let project_rate = self.doc.meta.sample_rate.value();
        let duration_samples = (probe.duration_seconds * f64::from(project_rate))
            .round()
            .max(1.0) as u64;
        let asset_id = Id::<notwig_core::ids::Asset>::from_uuid(Uuid::new_v4());
        let pending = self.bundle.is_none();
        let target = if let Some(bundle) = self.bundle.clone() {
            bundle
        } else {
            let dir = std::env::temp_dir().join(format!("notwig-import-{asset_id}"));
            std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
            self.pending_import_dirs.insert(asset_id, dir.clone());
            dir
        };
        let format = source
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("audio")
            .to_ascii_lowercase();
        let entry = notwig_core::AssetEntry {
            id: asset_id,
            kind: notwig_core::AssetKind::Audio,
            uri: format!("pending/{asset_id}.{format}"),
            original_name: source
                .file_name()
                .map(|name| name.to_string_lossy().into_owned()),
            hash: None,
            format,
            sample_rate: Some(project_rate),
            channels: Some(probe.channels),
            duration_samples: Some(duration_samples),
            original_sample_rate: Some(probe.sample_rate),
            original_channels: Some(probe.channels),
        };
        self.apply(CommandPayload::RegisterAsset { asset_id, entry })?;
        let placed = match placement {
            AudioImportPlacement::Clip { track_id, start } => {
                self.insert_audio_clip_at(track_id, asset_id, start)
            }
            AudioImportPlacement::Slice { clip_id, at } => {
                self.add_audio_slice(clip_id, asset_id, at)
            }
        };
        if let Err(err) = placed {
            if pending {
                self.pending_import_dirs.remove(&asset_id);
                let _ = std::fs::remove_dir_all(&target);
            }
            return Err(err);
        }
        self.pending_audio_imports.insert(asset_id);
        Ok((asset_id, target))
    }

    pub fn complete_audio_import(
        &mut self,
        asset_id: AssetId,
        imported: notwig_project::ImportedAsset,
    ) -> Result<(), String> {
        self.pending_audio_imports.remove(&asset_id);
        if !self.doc.assets.assets.contains_key(&asset_id) {
            return Ok(());
        }
        self.peaks
            .insert(asset_id, compute_peaks(&imported.buffer, 2048));
        let duration = imported.entry.duration_samples.unwrap_or(0);
        self.assets.insert(asset_id, Arc::new(imported.buffer));
        self.apply(CommandPayload::ReplaceAsset {
            asset_id,
            entry: imported.entry,
        })?;
        let map = self.doc.tempo_map();
        let clip_ids: Vec<ClipId> = self
            .doc
            .arranger
            .clips
            .iter()
            .filter_map(|(id, clip)| match &clip.payload {
                ClipPayload::Audio(audio) if !audio.uses_slices() && audio.asset_id == asset_id => {
                    Some(*id)
                }
                _ => None,
            })
            .collect();
        for clip_id in clip_ids {
            let Some(clip) = self.doc.arranger.clips.get(&clip_id) else {
                continue;
            };
            let end = clip.range.start + map.samples_to_mus(duration);
            if let Ok(range) = notwig_core::RangeMus::new(clip.range.start, end) {
                self.apply(CommandPayload::EditClip {
                    clip_id,
                    range: Some(range),
                    name: None,
                    muted: None,
                    content_start: None,
                    content_length: None,
                    audio: Some(notwig_core::protocol::AudioEdit {
                        source_start_sample: None,
                        source_end_sample: Some(duration),
                        gain_db: None,
                        fade_in_samples: None,
                        fade_out_samples: None,
                    }),
                })?;
            }
        }
        self.prepare_asset();
        self.status = Some("Audio import finished.".into());
        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
#[allow(clippy::unused_self, clippy::unnecessary_wraps)]
impl Workspace {
    pub(super) fn load_assets(&mut self) {}

    pub(super) fn persist_asset_file(
        &mut self,
        _asset_id: AssetId,
        _buffer: &AudioAssetBuffer,
    ) -> Result<(), String> {
        Ok(())
    }

    pub(super) fn prepare_asset(&mut self) {
        let assets = Arc::new(self.assets.clone());
        self.engine_control.data_mut().set_assets(assets);
        self.engine_control.flush();
    }

    pub(super) fn load_assets_from_files(&mut self, files: &notwig_project::BundleFiles) {
        self.assets.clear();
        self.peaks.clear();
        for entry in self.doc.assets.assets.values() {
            if entry.kind != notwig_core::AssetKind::Audio {
                continue;
            }
            let Some(bytes) = files.get(&entry.uri) else {
                self.status = Some(format!("Asset {} is missing from the bundle.", entry.id));
                continue;
            };
            match notwig_project::decode_audio_bytes(&entry.uri, bytes.to_vec()) {
                Ok(buffer) => {
                    self.peaks.insert(entry.id, compute_peaks(&buffer, 2048));
                    self.assets.insert(entry.id, Arc::new(buffer));
                }
                Err(err) => {
                    self.status = Some(format!("Asset {} unavailable: {err}", entry.id));
                }
            }
        }
        self.prepare_asset();
    }

    pub fn import_audio_bytes(&mut self, name: &str, bytes: Vec<u8>) -> Result<AssetId, String> {
        let decoded =
            notwig_project::decode_audio_bytes(name, bytes).map_err(|err| err.to_string())?;
        if decoded.channels > 2 {
            return Err("more than two channels are not supported".into());
        }
        let project_rate = self.doc.meta.sample_rate.value();
        let converted = notwig_project::resample(
            &decoded,
            project_rate,
            notwig_project::ResampleQuality::High,
        )
        .map_err(|err| err.to_string())?;
        let channels = if converted.channels == 1 {
            2
        } else {
            converted.channels
        };
        let samples = if converted.channels == 1 {
            converted
                .samples
                .iter()
                .flat_map(|sample| [*sample, *sample])
                .collect()
        } else {
            converted.samples.clone()
        };
        let buffer = AudioAssetBuffer {
            sample_rate: project_rate,
            channels,
            samples,
        };
        let duration_samples = buffer.frame_count();
        let asset_id = Id::<notwig_core::ids::Asset>::from_uuid(Uuid::new_v4());
        let uri = format!("assets/{asset_id}.wav");
        self.peaks.insert(asset_id, compute_peaks(&buffer, 2048));
        self.assets.insert(asset_id, Arc::new(buffer));
        let entry = notwig_core::AssetEntry {
            id: asset_id,
            kind: notwig_core::AssetKind::Audio,
            uri,
            original_name: Some(name.to_owned()),
            hash: None,
            format: "wav".into(),
            sample_rate: Some(project_rate),
            channels: Some(channels),
            duration_samples: Some(duration_samples),
            original_sample_rate: Some(decoded.sample_rate),
            original_channels: Some(decoded.channels),
        };
        self.apply(CommandPayload::RegisterAsset { asset_id, entry })?;
        self.prepare_asset();
        Ok(asset_id)
    }
}
