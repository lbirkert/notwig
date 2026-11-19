use std::path::Path;
use std::sync::Arc;

use notwig_core::protocol::CommandPayload;
use notwig_core::{AssetId, AudioAssetBuffer, ClipId, ClipPayload, Db, Id, TrackId};
use uuid::Uuid;

use super::Workspace;

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

    fn prepare_asset(&mut self) {
        let assets = Arc::new(self.assets.clone());
        self.engine_control.data_mut().set_assets(assets);
        self.engine_control.flush();
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
