use std::path::PathBuf;

use crate::clock::{Duration, Instant};
#[cfg(not(target_arch = "wasm32"))]
use crate::engine_process::EngineClient;
use notwig_core::AssetId;
use notwig_ui::NotificationKind;

use super::NotwigApp;

pub(super) struct AudioImportRequest {
    pub(super) asset_id: AssetId,
    pub(super) source: PathBuf,
    pub(super) target: PathBuf,
    pub(super) project_rate: u32,
}

#[cfg(not(target_arch = "wasm32"))]
fn audio_import_notification_tag(asset_id: AssetId) -> String {
    format!("audio-import:{asset_id}")
}

const AUDIO_DEVICE_SCAN_INTERVAL: Duration = Duration::from_millis(1000);
pub(super) const AUDIO_RESTART_INITIAL_DELAY: Duration = Duration::from_millis(500);
const AUDIO_RESTART_MAX_DELAY: Duration = Duration::from_secs(5);

#[derive(Default)]
pub(super) struct MeterSmoother {
    values: std::collections::BTreeMap<notwig_core::TrackId, notwig_ui::MeterValues>,
    last: Option<Instant>,
}

impl MeterSmoother {
    pub(super) fn update(
        &mut self,
        target: &std::collections::BTreeMap<notwig_core::TrackId, notwig_ui::MeterValues>,
        now: Instant,
    ) {
        let dt = self
            .last
            .map_or(0.0, |last| now.duration_since(last).as_secs_f32().min(0.25));
        self.last = Some(now);
        if target.is_empty() {
            self.values.clear();
            return;
        }
        let release = (-dt.max(0.0) / 0.12).exp();
        let mut next = std::collections::BTreeMap::new();
        for (track_id, target) in target {
            let current = self.values.get(track_id).copied().unwrap_or_default();
            next.insert(
                *track_id,
                notwig_ui::MeterValues {
                    peak_l: smooth_meter(current.peak_l, target.peak_l, release),
                    peak_r: smooth_meter(current.peak_r, target.peak_r, release),
                    rms_l: smooth_meter(current.rms_l, target.rms_l, release),
                    rms_r: smooth_meter(current.rms_r, target.rms_r, release),
                },
            );
        }
        self.values = next;
    }

    pub(super) fn values(
        &self,
    ) -> &std::collections::BTreeMap<notwig_core::TrackId, notwig_ui::MeterValues> {
        &self.values
    }
}

fn smooth_meter(current: f32, target: f32, release: f32) -> f32 {
    if target >= current {
        target
    } else {
        target + (current - target) * release
    }
}

impl NotwigApp {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn restart_audio(&mut self) {
        self.audio = None;
        let rate = self.workspace.doc.meta.sample_rate.value();
        match EngineClient::start(rate, &self.audio_settings) {
            Ok(audio) => {
                self.workspace.set_engine_process(Some(audio.handle()));
                self.audio = Some(audio);
                self.workspace.engine_audio_restarted();
                self.sync_recording_inputs();
                self.engine_stopped = false;
                self.clear_audio_device_lost(true);
            }
            Err(err) => {
                self.workspace.set_engine_process(None);
                self.audio_start_failed(err);
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) fn restart_audio(&mut self) {
        self.audio = None;
        let rate = self.workspace.doc.meta.sample_rate.value();
        match crate::audio::AudioService::start_with(
            self.workspace.engine_control_handle(),
            rate,
            &self.audio_settings,
        ) {
            Ok(audio) => {
                self.audio = Some(audio);
                self.workspace.engine_audio_restarted();
                self.sync_recording_inputs();
                self.engine_stopped = false;
                self.clear_audio_device_lost(true);
            }
            Err(err) => {
                self.audio_start_failed(err);
            }
        }
        self.audio_options = super::platform::available_audio_options();
    }

    fn audio_start_failed(&mut self, err: String) {
        if self.audio_device_lost || !self.configured_output_available() {
            self.enter_audio_device_lost(format!(
                "Audio output \"{}\" is unavailable: {err}",
                self.configured_output_label()
            ));
            self.schedule_audio_restart_after_failure();
        } else {
            self.workspace.error = Some(err.clone());
            self.workspace.status = Some(err.clone());
            self.notify_error(format!("Could not start the audio engine: {err}"));
        }
    }

    fn configured_output_label(&self) -> String {
        if self.audio_settings.device.is_empty()
            || self.audio_settings.device == crate::audio::SYSTEM_OUT_DEVICE_ID
        {
            "System Out".to_string()
        } else {
            self.audio_settings.device.clone()
        }
    }

    fn uses_system_output(&self) -> bool {
        self.audio_settings.device.is_empty()
            || self.audio_settings.device == crate::audio::SYSTEM_OUT_DEVICE_ID
    }

    fn configured_output_available(&self) -> bool {
        crate::audio::resolve_output_device_name(
            &self.audio_settings.driver,
            &self.audio_settings.device,
        )
        .is_ok()
    }

    fn rescan_audio_devices(&mut self) {
        self.audio_options = super::platform::available_audio_options();
    }

    fn enter_audio_device_lost(&mut self, message: String) {
        let was_lost = self.audio_device_lost;
        self.audio_device_lost = true;
        self.audio_device_lost_message = Some(message.clone());
        if !was_lost {
            self.next_audio_device_scan = Instant::now();
            self.audio_restart_backoff = AUDIO_RESTART_INITIAL_DELAY;
            self.audio_restart_at = Some(Instant::now() + AUDIO_RESTART_INITIAL_DELAY);
            if self.workspace.doc.transport.playing || self.recording.is_some() {
                self.handle_intent(notwig_ui::UiIntent::Pause);
            }
            self.rescan_audio_devices();
            self.workspace.error = None;
            self.ui
                .state
                .notify(NotificationKind::Warning, message.clone());
        }
        self.workspace.status = Some(message);
    }

    fn clear_audio_device_lost(&mut self, notify: bool) {
        if !self.audio_device_lost {
            return;
        }
        self.audio_device_lost = false;
        self.audio_device_lost_message = None;
        self.audio_restart_at = None;
        self.audio_restart_backoff = AUDIO_RESTART_INITIAL_DELAY;
        if notify {
            self.notify_info("Audio output restored.".into());
        }
    }

    fn schedule_audio_restart_after_failure(&mut self) {
        self.audio_restart_at = Some(Instant::now() + self.audio_restart_backoff);
        self.audio_restart_backoff = (self.audio_restart_backoff * 2).min(AUDIO_RESTART_MAX_DELAY);
    }

    fn restart_audio_for_output_change(&mut self) {
        let was_lost = self.audio_device_lost;
        self.restart_audio();
        if self.audio.is_some() && !was_lost {
            self.notify_info("Audio output changed.".into());
        }
    }

    pub(super) fn poll_audio_device_state(&mut self, snapshot: &crate::audio::AudioSnapshot) {
        if snapshot.device_lost {
            let reason = snapshot
                .device_lost_reason
                .as_deref()
                .map_or(String::new(), |reason| format!(": {reason}"));
            self.enter_audio_device_lost(format!(
                "Audio output \"{}\" is unavailable{reason}",
                snapshot.device_name
            ));
        } else if snapshot.device_changed && !self.audio_device_lost {
            self.rescan_audio_devices();
            self.restart_audio_for_output_change();
            return;
        }

        let now = Instant::now();
        if now < self.next_audio_device_scan {
            return;
        }
        self.next_audio_device_scan = now + AUDIO_DEVICE_SCAN_INTERVAL;
        self.rescan_audio_devices();

        if self.audio_device_lost {
            if self
                .audio_restart_at
                .is_none_or(|restart_at| now >= restart_at)
                && self.configured_output_available()
            {
                self.restart_audio_for_output_change();
            } else if !self.configured_output_available() {
                self.audio_restart_at = None;
            }
            return;
        }

        if snapshot.running
            && self.uses_system_output()
            && let Ok(default_name) = crate::audio::resolve_output_device_name(
                &self.audio_settings.driver,
                &self.audio_settings.device,
            )
            && !snapshot.device_name.is_empty()
            && default_name != snapshot.device_name
        {
            self.restart_audio_for_output_change();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn audio_snapshot(&self) -> crate::audio::AudioSnapshot {
        self.audio
            .as_ref()
            .map(EngineClient::snapshot)
            .unwrap_or_default()
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) fn audio_snapshot(&self) -> crate::audio::AudioSnapshot {
        self.audio
            .as_ref()
            .map(crate::audio::AudioService::snapshot)
            .unwrap_or_default()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn poll_audio_imports(&mut self) {
        while let Ok((asset_id, result)) = self.audio_import_rx.try_recv() {
            self.ui
                .state
                .remove_notification_by_tag(&audio_import_notification_tag(asset_id));
            match result {
                Ok(imported) => {
                    if let Err(err) = self.workspace.complete_audio_import(asset_id, imported) {
                        self.notify_error(err);
                    }
                }
                Err(err) => self.notify_error(format!("Audio import failed: {err}")),
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn poll_audio_analysis(&mut self) {
        while let Ok((path, duration)) = self.audio_analysis_rx.try_recv() {
            self.audio_analysis_pending.remove(&path);
            self.audio_durations.insert(path, duration);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn queue_audio_import(
        &mut self,
        asset_id: AssetId,
        source: PathBuf,
        target: PathBuf,
    ) {
        let project_rate = self.workspace.doc.meta.sample_rate.value();
        let _ = self.audio_import_tx.send(AudioImportRequest {
            asset_id,
            source,
            target,
            project_rate,
        });
        self.ui.state.notify_persistent_indeterminate(
            NotificationKind::Info,
            "Analyzing audio…",
            audio_import_notification_tag(asset_id),
        );
        self.workspace.status = Some("Analyzing audio…".into());
    }
}
