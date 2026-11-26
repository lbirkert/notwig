use std::path::PathBuf;
#[cfg(target_arch = "wasm32")]
use std::sync::Arc;

use notwig_core::{ProjectDoc, TrackId};

use crate::drop_target::DragPreview;
use crate::intent::{DropAudioTarget, UiIntent};
use crate::runtime::RuntimeView;
use crate::state::NotificationKind;
use crate::widgets::beats_to_time;

use super::NotwigUi;

impl NotwigUi {
    fn clip_editor_drop_point(
        &self,
        doc: &ProjectDoc,
        pointer: egui::Pos2,
    ) -> Option<(notwig_core::ClipId, notwig_core::TimeMus)> {
        let state = self.clip_editor.as_ref()?;
        if !self.clip_editor_content_rect.contains(pointer) {
            return None;
        }
        let clip = doc.arranger.clips.get(&state.clip_id)?;
        let ppb = state.viewport.pixels_per_beat.clamp(
            crate::state::Viewport::MIN_PPB,
            crate::state::Viewport::MAX_PPB,
        );
        let snap = self.state.snap;
        let absolute = snap.snap(
            state
                .viewport
                .x_to_beat_unbounded(pointer.x, self.clip_editor_content_rect.left()),
            ppb,
            doc.meta.bar_length(),
            false,
        );
        let at = absolute - clip.range.start;
        Some((state.clip_id, at))
    }

    fn pointer_drop_location(
        &self,
        doc: &ProjectDoc,
        pointer: egui::Pos2,
    ) -> Option<(DropAudioTarget, notwig_core::TimeMus)> {
        if let Some((clip_id, at)) = self.clip_editor_drop_point(doc, pointer) {
            return Some((DropAudioTarget::ClipEditor { clip_id }, at));
        }
        if !self.arranger_rect.contains(pointer) {
            return None;
        }
        let target =
            crate::drop_target::resolve_drop_target(doc, &self.last_track_rows, pointer.y)?;
        let snap = self.state.snap;
        let at = snap.snap(
            self.state.viewport.x_to_beat(pointer.x, self.timeline_left),
            self.state.viewport.pixels_per_beat,
            doc.meta.bar_length(),
            false,
        );
        Some((target, at))
    }

    pub(super) fn update_drag_preview(
        &mut self,
        ctx: &egui::Context,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
    ) {
        let hovered: Vec<PathBuf> = ctx.input(|input| {
            input
                .raw
                .hovered_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect()
        });
        let Some(path) = hovered.first().cloned() else {
            self.drag_preview = None;
            return;
        };
        let Some(pointer) = self
            .last_pointer
            .or_else(|| ctx.input(|input| input.pointer.latest_pos()))
        else {
            self.drag_preview = None;
            return;
        };
        let Some((target, at)) = self.pointer_drop_location(doc, pointer) else {
            self.drag_preview = None;
            return;
        };
        let known = runtime.audio_durations.get(&path).copied();
        let seconds = known.flatten().or_else(|| {
            notwig_project::probe_audio(&path)
                .ok()
                .map(|probe| probe.duration_seconds)
        });
        let analyzing = seconds.is_none() && runtime.audio_analysis_pending.contains(&path);
        if seconds.is_none() && !analyzing && !runtime.audio_durations.contains_key(&path) {
            self.emit(UiIntent::AnalyzeAudioDuration(path.clone()));
        }
        let beats = seconds.map_or(4.0, |seconds| {
            (seconds * doc.meta.tempo_bpm / 60.0).max(1.0 / 16.0)
        });
        let duration = beats_to_time(beats);
        self.drag_preview = Some(DragPreview {
            path,
            target,
            at,
            duration,
            analyzing: seconds.is_none(),
        });
    }

    pub(super) fn handle_dropped_files(&mut self, ctx: &egui::Context, doc: &ProjectDoc) {
        let dropped = ctx.input(|input| input.raw.dropped_files.clone());
        if dropped.is_empty() {
            return;
        }

        if self.settings_open
            && self.settings_category == crate::settings::SettingsCategory::Extensions
        {
            #[cfg(not(target_arch = "wasm32"))]
            {
                let zips: Vec<PathBuf> = dropped
                    .iter()
                    .filter_map(|file| file.path.clone())
                    .filter(|path| is_zip_path(path))
                    .collect();
                if !zips.is_empty() {
                    for path in zips {
                        self.emit(UiIntent::InstallExtensionZip { path });
                    }
                    return;
                }
            }
            #[cfg(target_arch = "wasm32")]
            {
                let zips: Vec<(String, Arc<[u8]>)> = dropped
                    .iter()
                    .filter(|file| is_zip_name(&file.name))
                    .filter_map(|file| {
                        file.bytes
                            .as_ref()
                            .map(|bytes| (file.name.clone(), Arc::clone(bytes)))
                    })
                    .collect();
                if !zips.is_empty() {
                    for (name, bytes) in zips {
                        self.emit(UiIntent::InstallExtensionZipBytes { name, bytes });
                    }
                    return;
                }
            }
        }

        let Some((first_target, at)) = self.drop_location(ctx, doc) else {
            self.state.notify(
                NotificationKind::Warning,
                "Drop audio files onto the timeline or clip editor.",
            );
            return;
        };
        let order: Vec<TrackId> = doc.track_order.depth_first();

        #[cfg(not(target_arch = "wasm32"))]
        {
            let files: Vec<PathBuf> = dropped
                .iter()
                .filter_map(|file| file.path.clone())
                .collect();
            if files.is_empty() {
                return;
            }
            for (file_index, file) in files.into_iter().enumerate() {
                let target = file_drop_target(&order, first_target, file_index);
                self.emit(UiIntent::DropAudio {
                    path: file,
                    target,
                    at,
                });
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            let files: Vec<(String, Arc<[u8]>)> = dropped
                .iter()
                .filter(|file| is_audio_name(&file.name) || file.mime.starts_with("audio/"))
                .filter_map(|file| {
                    file.bytes.as_ref().map(|bytes| {
                        let name = if file.name.is_empty() {
                            "dropped-audio".to_owned()
                        } else {
                            file.name.clone()
                        };
                        (name, Arc::clone(bytes))
                    })
                })
                .collect();
            if files.is_empty() {
                self.state.notify(
                    NotificationKind::Warning,
                    "The dropped files do not contain supported audio.",
                );
                return;
            }
            for (file_index, (name, bytes)) in files.into_iter().enumerate() {
                let target = file_drop_target(&order, first_target, file_index);
                self.emit(UiIntent::DropAudioBytes {
                    name,
                    bytes,
                    target,
                    at,
                });
            }
        }
    }

    fn drop_location(
        &self,
        ctx: &egui::Context,
        doc: &ProjectDoc,
    ) -> Option<(DropAudioTarget, notwig_core::TimeMus)> {
        let pointer = self
            .last_pointer
            .or_else(|| ctx.input(|input| input.pointer.latest_pos()));
        let from_pointer = pointer.and_then(|pointer| self.pointer_drop_location(doc, pointer));
        from_pointer.or_else(|| {
            self.drag_preview
                .as_ref()
                .map(|preview| (preview.target, preview.at))
        })
    }
}

fn file_drop_target(
    order: &[TrackId],
    first_target: DropAudioTarget,
    file_index: usize,
) -> DropAudioTarget {
    match (file_index, first_target) {
        (0, target) | (_, target @ DropAudioTarget::ClipEditor { .. }) => target,
        (_, DropAudioTarget::Track(track_id)) => {
            let index = order
                .iter()
                .position(|id| *id == track_id)
                .map_or(order.len(), |index| index + 1);
            DropAudioTarget::NewTrack {
                index: index + file_index - 1,
            }
        }
        (_, DropAudioTarget::NewTrack { index }) => DropAudioTarget::NewTrack {
            index: index + file_index,
        },
    }
}

#[cfg(target_arch = "wasm32")]
fn is_audio_name(name: &str) -> bool {
    matches!(
        name.rsplit_once('.')
            .map(|(_, extension)| extension.to_ascii_lowercase())
            .as_deref(),
        Some("wav" | "wave" | "aif" | "aiff" | "aifc" | "mp3" | "flac" | "ogg" | "oga")
    )
}

#[cfg(target_arch = "wasm32")]
fn is_zip_name(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("zip"))
}

#[cfg(not(target_arch = "wasm32"))]
fn is_zip_path(path: &std::path::Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("zip"))
}
