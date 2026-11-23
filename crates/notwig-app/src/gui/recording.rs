use std::sync::Arc;

use crate::clock::{Duration, Instant};
use notwig_core::{AudioAssetBuffer, AudioInputRef, CommandPayload, TimeMus, TrackId, TrackKind};
use notwig_ui::{AudioWaveform, RecordingTakeView};

use super::NotwigApp;

struct LiveRecordingTake {
    samples: Vec<f32>,
    waveform: Arc<AudioWaveform>,
    waveform_dirty: bool,
    last_waveform_build: Instant,
}

pub(super) struct ActiveRecording {
    started_at: TimeMus,
    sample_rate: u32,
    takes: std::collections::BTreeMap<TrackId, LiveRecordingTake>,
    pub(super) end: Option<TimeMus>,
    compensation_samples: usize,
    #[cfg(target_arch = "wasm32")]
    session: Option<crate::recording::RecordingSession>,
}

fn new_live_take(sample_rate: u32) -> LiveRecordingTake {
    LiveRecordingTake {
        samples: Vec::new(),
        waveform: Arc::new(notwig_ui::AudioWaveform::new(Arc::new(AudioAssetBuffer {
            sample_rate,
            channels: 2,
            samples: Vec::new(),
        }))),
        waveform_dirty: false,
        last_waveform_build: Instant::now(),
    }
}

fn append_take_samples(active: &mut ActiveRecording, track_id: TrackId, samples: &[f32]) {
    let sample_rate = active.sample_rate;
    let take = active
        .takes
        .entry(track_id)
        .or_insert_with(|| new_live_take(sample_rate));
    take.samples.extend_from_slice(samples);
    take.waveform_dirty = true;
}

pub(super) fn append_captures(active: &mut ActiveRecording, captures: Vec<(TrackId, Vec<f32>)>) {
    let skip = active.compensation_samples;
    let mut skipped = 0;
    for (track_id, samples) in captures {
        let skip_samples = skip.min(samples.len());
        append_take_samples(active, track_id, &samples[skip_samples..]);
        skipped = skipped.max(skip_samples);
    }
    active.compensation_samples = active.compensation_samples.saturating_sub(skipped);
}

impl NotwigApp {
    fn armed_input_requests(&self) -> Vec<(TrackId, AudioInputRef)> {
        self.workspace
            .doc
            .tracks
            .values()
            .filter(|track| track.kind == TrackKind::Audio && track.record_armed)
            .filter_map(|track| {
                track
                    .io
                    .audio_input
                    .clone()
                    .filter(|input| {
                        input.driver == self.audio_settings.driver
                            && input.device == self.audio_settings.device
                    })
                    .map(|input| (track.id, input))
            })
            .collect()
    }

    pub(super) fn sync_recording_inputs(&mut self) {
        if self.recording.is_some() {
            return;
        }
        let requests = self.armed_input_requests();
        if self.workspace.doc.transport.recording
            && self.workspace.doc.transport.playing
            && !requests.is_empty()
        {
            self.begin_recording_capture(None);
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let Some(audio) = &self.audio else {
                return;
            };
            if let Err(error) = audio.handle().configure_inputs(requests) {
                self.notify_error(error);
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let Some(audio) = &self.audio else {
                return;
            };
            match audio.configure_inputs(&requests) {
                Ok(routes) => self.workspace.set_input_routes(routes),
                Err(error) => self.notify_error(error),
            }
        }
    }

    pub(super) fn toggle_record(&mut self) {
        if self.workspace.doc.transport.recording {
            let result = self
                .workspace
                .apply(CommandPayload::SetRecording { recording: false });
            self.stop_recording_capture();
            let _ = self.report(result);
            return;
        }

        let was_playing = self.workspace.doc.transport.playing;
        let play_start = self.workspace.doc.transport.playhead;
        if !was_playing {
            let result = self.workspace.play();
            if self.report(result).is_none() {
                return;
            }
        }
        let result = self
            .workspace
            .apply(CommandPayload::SetRecording { recording: true });
        if self.report(result).is_some() {
            self.begin_recording_capture((!was_playing).then_some(play_start));
        }
    }

    pub(super) fn disengage_recording(&mut self) {
        if self.recording.is_some() {
            self.stop_recording_capture();
        }
        if self.workspace.doc.transport.recording {
            let result = self
                .workspace
                .apply(CommandPayload::SetRecording { recording: false });
            let _ = self.report(result);
        }
    }

    pub(super) fn begin_recording_capture(&mut self, started_at: Option<TimeMus>) {
        if self.recording.is_some() {
            return;
        }
        let requests = self.armed_input_requests();
        if requests.is_empty() {
            return;
        }
        let snapshot = self.audio_snapshot();
        let started_at = started_at.unwrap_or_else(|| self.current_playhead(&snapshot));
        let sample_rate = self.workspace.doc.meta.sample_rate.value();
        let device_block = if snapshot.block_size > 0 {
            u64::from(snapshot.block_size)
        } else {
            u64::from(self.audio_settings.block_size)
        };
        let measured_latency = snapshot
            .dsp
            .input_latency_samples
            .saturating_add(snapshot.dsp.output_latency_samples);
        let compensation_frames = measured_latency
            .max(device_block)
            .min(u64::from(sample_rate))
            .max(1);
        let compensation_samples = usize::try_from(compensation_frames)
            .unwrap_or(1)
            .saturating_mul(2);
        let mut takes = std::collections::BTreeMap::new();
        for (track_id, _) in &requests {
            takes.insert(*track_id, new_live_take(sample_rate));
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let Some(audio) = &self.audio else {
                self.notify_error("The audio engine is not running.".into());
                return;
            };
            if let Err(error) = audio.handle().configure_inputs(requests) {
                self.notify_error(error);
                return;
            }
            if let Err(error) = audio.handle().start_recording() {
                self.notify_error(error);
                return;
            }
            self.recording = Some(ActiveRecording {
                started_at,
                sample_rate,
                takes,
                end: None,
                compensation_samples,
            });
        }
        #[cfg(target_arch = "wasm32")]
        {
            let Some(audio) = &self.audio else {
                self.notify_error("The audio engine is not running.".into());
                return;
            };
            let routes = match audio.configure_inputs(&requests) {
                Ok(routes) => {
                    self.workspace.set_input_routes(routes.clone());
                    routes
                }
                Err(error) => {
                    self.notify_error(error);
                    return;
                }
            };
            let session = audio.begin_recording(routes, sample_rate);
            self.recording = Some(ActiveRecording {
                started_at,
                sample_rate,
                takes,
                end: None,
                compensation_samples,
                session: Some(session),
            });
        }
    }

    fn stop_recording_capture(&mut self) {
        let snapshot = self.audio_snapshot();
        let end = self.current_playhead(&snapshot);
        let Some(active) = self.recording.as_mut() else {
            return;
        };
        active.end = Some(end);
        #[cfg(not(target_arch = "wasm32"))]
        {
            let Some(audio) = &self.audio else {
                let active = self.recording.take().expect("recording was just checked");
                self.finalize_live_recording(active, end);
                return;
            };
            if let Err(error) = audio.handle().stop_recording() {
                let active = self.recording.take().expect("recording was just checked");
                self.finalize_live_recording(active, end);
                self.notify_error(error);
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let Some(mut active) = self.recording.take() else {
                return;
            };
            if let Some(session) = active.session.take() {
                match session.finish() {
                    Ok(captures) => {
                        let captures = captures
                            .into_iter()
                            .map(|(track_id, buffer)| (track_id, buffer.samples))
                            .collect();
                        append_captures(&mut active, captures);
                    }
                    Err(error) => self.notify_error(error),
                }
            }
            self.finalize_live_recording(active, end);
        }
    }

    pub(super) fn append_recording_chunk(
        &mut self,
        sample_rate: u32,
        captures: Vec<(TrackId, Vec<f32>)>,
    ) {
        let Some(active) = self.recording.as_mut() else {
            return;
        };
        active.sample_rate = sample_rate;
        append_captures(active, captures);
    }

    pub(super) fn refresh_recording_waveforms(&mut self) {
        let Some(active) = self.recording.as_mut() else {
            return;
        };
        let now = Instant::now();
        for take in active.takes.values_mut() {
            let first_build = take.waveform.frame_count() == 0;
            if !take.waveform_dirty
                || (!first_build
                    && now.duration_since(take.last_waveform_build) < Duration::from_millis(100))
            {
                continue;
            }
            take.last_waveform_build = now;
            take.waveform_dirty = false;
            take.waveform = Arc::new(notwig_ui::AudioWaveform::new(Arc::new(AudioAssetBuffer {
                sample_rate: active.sample_rate,
                channels: 2,
                samples: take.samples.clone(),
            })));
        }
    }

    pub(super) fn recording_take_views(&self, playhead: TimeMus) -> Vec<RecordingTakeView> {
        let Some(active) = &self.recording else {
            return Vec::new();
        };
        let map = self.workspace.doc.tempo_map();
        active
            .takes
            .iter()
            .filter_map(|(track_id, take)| {
                let sample_end =
                    active.started_at + map.samples_to_mus(take.waveform.frame_count());
                let end = if playhead > active.started_at {
                    playhead.max(sample_end)
                } else {
                    sample_end
                };
                if end <= active.started_at {
                    return None;
                }
                Some(RecordingTakeView {
                    track_id: *track_id,
                    start: active.started_at,
                    end,
                    waveform: Arc::clone(&take.waveform),
                })
            })
            .collect()
    }

    pub(super) fn finalize_live_recording(&mut self, active: ActiveRecording, end: TimeMus) {
        self.recording = None;
        if end <= active.started_at {
            return;
        }
        let frames = usize::try_from(
            self.workspace
                .doc
                .tempo_map()
                .mus_to_samples(end - active.started_at),
        )
        .unwrap_or(0);
        let target_samples = frames.saturating_mul(2);
        let mut last_clip = None;
        for (track_id, mut take) in active.takes {
            if take.samples.is_empty() || target_samples == 0 {
                continue;
            }
            take.samples.resize(target_samples, 0.0);
            take.samples.truncate(target_samples);
            let buffer = AudioAssetBuffer {
                sample_rate: active.sample_rate,
                channels: 2,
                samples: take.samples,
            };
            match self
                .workspace
                .finish_recording(track_id, active.started_at, buffer)
            {
                Ok(clip_id) => last_clip = Some((track_id, clip_id)),
                Err(error) => self.notify_error(error),
            }
        }
        if let Some((track_id, clip_id)) = last_clip {
            self.ui.state.selection.select_track(track_id, false);
            self.ui
                .state
                .selection
                .select_clip(clip_id, track_id, false);
        }
        self.fallback_playhead = self.workspace.doc.transport.playhead.as_f64();
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) fn poll_web_recording(&mut self) {
        let Some(active) = &self.recording else {
            return;
        };
        let Some(session) = &active.session else {
            return;
        };
        session.poll();
        let sample_rate = active.sample_rate;
        let captures = session.take_captured();
        if captures.is_empty() {
            return;
        }
        self.append_recording_chunk(sample_rate, captures.into_iter().collect());
    }
}
