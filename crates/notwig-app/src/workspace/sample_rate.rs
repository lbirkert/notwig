use std::sync::Arc;

use notwig_core::AssetId;

use super::Workspace;

impl Workspace {
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
            let converted = Arc::new(converted);
            self.insert_waveform(asset_id, Arc::clone(&converted));
            self.persist_asset_file(asset_id, &converted)?;
            if let Some(entry) = self.doc.assets.assets.get_mut(&asset_id) {
                entry.sample_rate = Some(rate.value());
                entry.duration_samples = Some(converted.frame_count());
            }
            self.assets.insert(asset_id, converted);
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
}

fn scale_sample(value: u64, ratio: f64) -> u64 {
    (value as f64 * ratio).round() as u64
}
