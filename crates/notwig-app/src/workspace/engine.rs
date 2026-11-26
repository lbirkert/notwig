use std::sync::Arc;

use crate::engine_build::EngineBuildRequest;
#[cfg(not(target_arch = "wasm32"))]
use crate::engine_process::EngineClientHandle;
use notwig_core::dsp::EngineCaps;
use notwig_core::protocol::PlaybackSchedule;
use notwig_engine::{
    ControlHandle, DeviceBypassUpdate, Engine, ParamUpdate, PreparedRuntime, TrackStateUpdate,
    TransportUpdate,
};

use super::{OFFLINE_BLOCK_SIZE, Workspace};

impl Workspace {
    #[allow(dead_code)]
    pub fn rebuild_engine(&mut self) -> Result<(), String> {
        let prepared = self
            .engine_build_request(self.live_block_size())
            .prepare()?;
        self.install_prepared_runtime(prepared);
        Ok(())
    }

    /// Publishes a gain-curve preview without changing the document or undo
    /// history. The final committed command follows on pointer release.
    pub fn preview_clip_gain_curve(
        &mut self,
        clip_id: notwig_core::ClipId,
        curve: Option<notwig_core::Curve>,
    ) -> Result<(), String> {
        if let Some(curve) = &curve {
            curve.validate().map_err(|error| error.to_string())?;
        }
        let mut schedule = PlaybackSchedule::from_doc(&self.doc);
        for clips in schedule.tracks.values_mut() {
            for clip in clips {
                if clip.id != clip_id {
                    continue;
                }
                let notwig_core::ClipPayload::Audio(audio) = &mut clip.payload else {
                    continue;
                };
                audio.gain_curve.clone_from(&curve);
            }
        }
        self.engine_control
            .data_mut()
            .set_schedule(Arc::new(schedule));
        Ok(())
    }

    pub(super) fn preserved_playhead_samples(&self) -> u64 {
        let engine_playhead = self
            .engine_control
            .handle()
            .playhead_samples()
            .max(self.engine_playhead_samples);
        if engine_playhead > 0 {
            engine_playhead
        } else {
            self.doc
                .tempo_map()
                .mus_to_samples(self.doc.transport.playhead)
        }
    }

    #[allow(dead_code)]
    pub(crate) fn set_engine_playhead_samples(&mut self, samples: u64) {
        if samples > 0 {
            self.engine_playhead_samples = samples;
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

    fn live_block_size(&self) -> usize {
        self.audio_block_size.max(16) as usize
    }

    fn build_engine(&self) -> Result<Engine, String> {
        self.engine_build_request(self.live_block_size()).build()
    }

    /// Offline rendering is not bound by device latency, so process in large
    /// blocks to amortise per-block overhead instead of inheriting the live
    /// audio buffer size.
    pub(super) fn build_offline_engine(&self) -> Result<Engine, String> {
        self.engine_build_request(OFFLINE_BLOCK_SIZE).build()
    }

    pub(super) fn engine_build_request(&self, max_block_size: usize) -> EngineBuildRequest {
        let sample_rate = self.doc.meta.sample_rate.value();
        EngineBuildRequest {
            doc: self.doc.clone(),
            caps: EngineCaps {
                sample_rate,
                max_block_size,
                channels: 2,
            },
            registry: self.device_registry(),
            extension_factory: self.extension_factory.clone(),
            plugin_descriptors: self.plugin_descriptors.clone(),
            plugin_states: self.plugin_states.clone(),
            assets: self.assets.clone(),
            sample_rate,
            max_block_size,
            monitors: Arc::clone(&self.monitors),
            extension_runtime: self.extension_runtime.clone(),
        }
    }

    pub fn set_low_latency_monitoring(&mut self, enabled: bool) -> bool {
        let changed = self.low_latency_monitoring != enabled;
        self.low_latency_monitoring = enabled;
        changed
    }

    pub fn request_engine_rebuild(&mut self) {
        tracing::debug!(
            sample_rate = self.doc.meta.sample_rate.value(),
            block_size = self.live_block_size(),
            "Rebuilding engine"
        );
        self.status = Some("Loading instruments and effects…".into());
        let request = self.engine_build_request(self.live_block_size());
        #[cfg(not(target_arch = "wasm32"))]
        if self.engine_process.is_some() {
            if let Err(err) = self.engine_process_build.request(request) {
                self.error = Some(err.clone());
                self.status = Some(err);
            }
            return;
        }
        if let Err(err) = self.engine_build.request(request) {
            self.error = Some(err.clone());
            self.status = Some(err);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[allow(dead_code)]
    pub(crate) fn set_engine_process(&mut self, handle: Option<EngineClientHandle>) {
        self.engine_process.clone_from(&handle);
        let sink = handle.map(|handle| {
            Box::new(move |update| {
                let _ = handle.send_control(update);
            }) as Box<dyn Fn(notwig_engine::EngineControlUpdate) + Send>
        });
        self.engine_control.set_remote_sink(sink);
    }

    #[allow(dead_code)]
    pub(crate) fn set_extension_engine_runtime(
        &mut self,
        runtime: Option<crate::engine_build::EngineExtensionRuntime>,
    ) {
        self.extension_runtime = runtime;
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[allow(dead_code)]
    pub(crate) fn engine_process_build_finished(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.engine_ready = true;
                self.status = Some("Instruments and effects loaded.".into());
            }
            Err(error) => {
                self.error = Some(error.clone());
                self.status = Some(format!("Failed to load instruments and effects: {error}"));
            }
        }
    }

    pub fn engine_building(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        if self.engine_process_build.building() {
            return true;
        }
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
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(outcome) = self.engine_process_build.poll() {
            match outcome {
                Ok(build) => {
                    if let Some(handle) = &self.engine_process {
                        if let Err(error) = handle.send_build(build) {
                            self.error = Some(error.clone());
                            self.status = Some(format!("Failed to send the engine build: {error}"));
                        }
                    }
                }
                Err(err) => {
                    self.error = Some(err.clone());
                    self.status = Some(format!("Failed to load instruments and effects: {err}"));
                }
            }
            return true;
        }
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
        tracing::debug!("Engine runtime ready; publishing swap");
        // Always republish transport when a runtime is installed. Tempo and
        // sample-rate changes rebuild the runtime and must also refresh the
        // sample positions of a musical loop range. The exchange keeps a pending
        // prepared runtime across intervening publishes, so reporting readiness
        // here is safe: the runtime will be installed unless a newer one
        // supersedes it.
        let transport = Some(self.transport_update(self.preserved_playhead_samples()));
        self.engine_ready = true;
        self.publish_engine_control(transport, false, Some(runtime));
    }

    pub(super) fn sync_engine(&mut self) {
        self.publish_engine_control(None, false, None);
    }

    /// Publish a metronome-only change. This deliberately omits the transport
    /// so toggling the click cannot move the playhead or clear sustained notes.
    pub(super) fn publish_metronome(&mut self) {
        let metronome = notwig_core::protocol::MetronomeState {
            enabled: self.doc.transport.metronome_enabled,
            beats_per_bar: self.doc.meta.time_sig_num,
            beat_length: self.doc.meta.beat_length(),
            bar_length: self.doc.meta.bar_length(),
        };
        self.engine_control.data_mut().set_metronome(metronome);
        self.engine_control.flush();
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
                beat_length: self.doc.meta.beat_length(),
                bar_length: self.doc.meta.bar_length(),
            }),
        }
    }

    pub(super) fn publish_engine_control(
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
        data.set_low_latency_monitoring(self.low_latency_monitoring);
        for track in self.doc.tracks.values() {
            data.push_track_state(TrackStateUpdate {
                track_id: track.id,
                mute: track.mixer.mute,
                solo_effective: self.doc.effective_solo(track.id),
                gain_db: track.mixer.volume_db,
                pan: track.mixer.pan,
                monitor: track.io.monitor,
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
            data.push_device_bypass(DeviceBypassUpdate {
                device_id: device.id,
                bypass: device.bypass,
            });
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
}
