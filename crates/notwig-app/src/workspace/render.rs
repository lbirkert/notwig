use std::path::{Path, PathBuf};

#[cfg(not(target_arch = "wasm32"))]
use crate::engine_build::EngineBuildRequest;
use notwig_core::protocol::EngineMsg;
use notwig_core::{ProjectDoc, Rational};
use notwig_engine::Engine;

#[cfg(not(target_arch = "wasm32"))]
use super::OFFLINE_BLOCK_SIZE;
use super::Workspace;

impl Workspace {
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
        let mut engine = self.build_offline_engine()?;
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
        render_stems(&self.doc, directory, start, end, options, || {
            self.build_offline_engine()
        })
    }

    /// Captures the immutable engine inputs needed to render stems on another
    /// thread. Call [`StemsRenderJob::render`] off the UI thread.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn stems_render_job(
        &self,
        directory: &Path,
        start: notwig_core::TimeMus,
        end: notwig_core::TimeMus,
        options: notwig_project::ExportOptions,
    ) -> StemsRenderJob {
        StemsRenderJob {
            template: self.engine_build_request(OFFLINE_BLOCK_SIZE),
            directory: directory.to_path_buf(),
            start,
            end,
            options,
        }
    }
}

fn render_stems(
    doc: &ProjectDoc,
    directory: &Path,
    start: notwig_core::TimeMus,
    end: notwig_core::TimeMus,
    options: notwig_project::ExportOptions,
    mut build_engine: impl FnMut() -> Result<Engine, String>,
) -> Result<Vec<(PathBuf, usize)>, String> {
    std::fs::create_dir_all(directory).map_err(|err| err.to_string())?;
    let map = doc.tempo_map();
    let start_sample = map.mus_to_samples(start.max(notwig_core::TimeMus::zero()));
    let end_sample = map
        .mus_to_samples(end.max(start + notwig_core::TimeMus::new(1, 16)))
        .max(start_sample + 1);
    let master = doc.master_track();
    let mut written = Vec::new();
    for track in doc
        .tracks
        .values()
        .filter(|track| track.kind != notwig_core::TrackKind::Master)
    {
        let mut engine = build_engine()?;
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
        for state in doc.tracks.values() {
            let ancestor = is_ancestor(doc, state.id, track.id);
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
            doc.meta.sample_rate.value(),
            2,
            &output,
            options,
        )
        .map_err(|err| err.to_string())?;
        written.push((path, output.len() / 2));
    }
    Ok(written)
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

/// Owned snapshot of the inputs needed to render stems off the UI thread.
#[cfg(not(target_arch = "wasm32"))]
pub struct StemsRenderJob {
    template: EngineBuildRequest,
    directory: PathBuf,
    start: notwig_core::TimeMus,
    end: notwig_core::TimeMus,
    options: notwig_project::ExportOptions,
}

#[cfg(not(target_arch = "wasm32"))]
impl StemsRenderJob {
    pub fn render(self) -> Result<Vec<(PathBuf, usize)>, String> {
        let Self {
            template,
            directory,
            start,
            end,
            options,
        } = self;
        render_stems(&template.doc, &directory, start, end, options, || {
            template.clone().build()
        })
    }
}
