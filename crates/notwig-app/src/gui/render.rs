use std::path::PathBuf;
use std::sync::mpsc;

use notwig_ui::NotificationKind;

use super::NotwigApp;

pub(super) struct RenderJob {
    pub(super) session: RenderSession,
    pub(super) output: Vec<f32>,
    pub(super) path: PathBuf,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(super) options: notwig_project::ExportOptions,
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) struct RenderEncodeResult {
    total_frames: usize,
    path: PathBuf,
    result: Result<(), String>,
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) struct StemsRenderResult {
    pub(super) directory: PathBuf,
    pub(super) result: Result<Vec<(PathBuf, usize)>, String>,
}

pub(super) const RENDER_ENCODING_NOTIFICATION_TAG: &str = "render-encoding";

use crate::workspace::RenderSession;

impl NotwigApp {
    pub(super) fn advance_render(&mut self) {
        let Some(mut job) = self.render_job.take() else {
            self.ui.render_progress = None;
            self.render_playhead = None;
            return;
        };
        let total = job.session.total_frames();
        let mut error = None;
        let mut budget = 8;
        while budget > 0 && job.output.len() / 2 < total {
            let frames = (total - job.output.len() / 2).min(4096);
            match job.session.render_block(frames) {
                Ok(block) => job.output.extend(block),
                Err(err) => {
                    error = Some(err);
                    break;
                }
            }
            budget -= 1;
        }
        if let Some(err) = error {
            self.notify_error(err);
            self.ui.render_progress = None;
            self.render_playhead = None;
            return;
        }
        let done = job.output.len() / 2;
        let progress = (done as f32 / total.max(1) as f32).min(1.0);
        self.ui.render_progress = Some(progress);
        self.render_playhead = Some(
            self.workspace
                .doc
                .tempo_map()
                .samples_to_mus(job.session.start_sample.saturating_add(done as u64)),
        );
        if done >= total {
            job.output.truncate(total * 2);
            #[cfg(not(target_arch = "wasm32"))]
            {
                let path = job.path;
                let sample_rate = job.session.sample_rate;
                let options = job.options;
                let output = job.output;
                let (tx, rx) = mpsc::channel();
                std::thread::spawn(move || {
                    let result =
                        notwig_project::write_audio_export(&path, sample_rate, 2, &output, options)
                            .map_err(|err| err.to_string());
                    let _ = tx.send(RenderEncodeResult {
                        total_frames: total,
                        path,
                        result,
                    });
                });
                self.render_encode_rx = Some(rx);
                self.ui.state.notify_persistent_indeterminate(
                    NotificationKind::Info,
                    "Encoding audio…",
                    RENDER_ENCODING_NOTIFICATION_TAG,
                );
            }
            #[cfg(target_arch = "wasm32")]
            {
                self.pending_web_encode = Some(job);
                self.ui.state.notify_persistent_indeterminate(
                    NotificationKind::Info,
                    "Encoding audio…",
                    RENDER_ENCODING_NOTIFICATION_TAG,
                );
            }
            self.ui.render_progress = None;
            self.render_playhead = None;
        } else {
            self.render_job = Some(job);
        }
    }

    pub(super) fn render_busy(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.render_job.is_some()
                || self.render_encode_rx.is_some()
                || self.stems_render_rx.is_some()
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.render_job.is_some() || self.pending_web_encode.is_some()
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn poll_render_encode(&mut self) {
        let received = match self.render_encode_rx.as_ref() {
            Some(rx) => rx.try_recv(),
            None => return,
        };
        match received {
            Ok(encoded) => {
                self.render_encode_rx = None;
                self.ui
                    .state
                    .remove_notification_by_tag(RENDER_ENCODING_NOTIFICATION_TAG);
                match encoded.result {
                    Ok(()) => self.notify_info(format!(
                        "Rendered {} frames to {}",
                        encoded.total_frames,
                        encoded.path.display()
                    )),
                    Err(err) => self.notify_error(err),
                }
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.render_encode_rx = None;
                self.ui
                    .state
                    .remove_notification_by_tag(RENDER_ENCODING_NOTIFICATION_TAG);
                self.notify_error("Audio encoding stopped unexpectedly.".into());
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn poll_stems_render(&mut self) {
        let received = match self.stems_render_rx.as_ref() {
            Some(rx) => rx.try_recv(),
            None => return,
        };
        match received {
            Ok(completed) => {
                self.stems_render_rx = None;
                self.ui
                    .state
                    .remove_notification_by_tag(RENDER_ENCODING_NOTIFICATION_TAG);
                match completed.result {
                    Ok(stems) => self.notify_info(format!(
                        "Rendered {} stems to {}",
                        stems.len(),
                        completed.directory.display()
                    )),
                    Err(err) => self.notify_error(err),
                }
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.stems_render_rx = None;
                self.ui
                    .state
                    .remove_notification_by_tag(RENDER_ENCODING_NOTIFICATION_TAG);
                self.notify_error("Stem rendering stopped unexpectedly.".into());
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) fn poll_web_encode(&mut self) {
        let Some(job) = self.pending_web_encode.take() else {
            return;
        };
        let total = job.session.total_frames();
        self.ui
            .state
            .remove_notification_by_tag(RENDER_ENCODING_NOTIFICATION_TAG);
        match notwig_project::write_wav_f32_bytes(job.session.sample_rate, 2, &job.output) {
            Ok(bytes) => {
                let name = job
                    .path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("mixdown.wav");
                match crate::web::storage::download_bytes(name, "audio/wav", &bytes) {
                    Ok(()) => self.notify_info(format!("Rendered {total} frames.")),
                    Err(error) => self.notify_error(error),
                }
            }
            Err(error) => self.notify_error(error.to_string()),
        }
    }
}

pub(super) fn export_options(options: notwig_ui::ExportOptions) -> notwig_project::ExportOptions {
    notwig_project::ExportOptions {
        format: match options.format {
            notwig_ui::ExportFormat::Wav => notwig_project::ExportFormat::Wav,
            notwig_ui::ExportFormat::Aiff => notwig_project::ExportFormat::Aiff,
            notwig_ui::ExportFormat::Flac => notwig_project::ExportFormat::Flac,
            notwig_ui::ExportFormat::Mp3 => notwig_project::ExportFormat::Mp3,
            notwig_ui::ExportFormat::Ogg => notwig_project::ExportFormat::Ogg,
        },
        bit_depth: match options.bit_depth {
            notwig_ui::ExportBitDepth::Int16 => notwig_project::ExportBitDepth::Int16,
            notwig_ui::ExportBitDepth::Int24 => notwig_project::ExportBitDepth::Int24,
            notwig_ui::ExportBitDepth::Float32 => notwig_project::ExportBitDepth::Float32,
        },
        dither: options.dither,
        normalize: options.normalize,
    }
}
