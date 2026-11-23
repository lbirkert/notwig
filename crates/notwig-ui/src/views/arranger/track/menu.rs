use notwig_core::{TrackId, TrackKind};

use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::shell::NotwigUi;

impl NotwigUi {
    pub(super) fn track_header_menu(
        &mut self,
        ui: &mut egui::Ui,
        track_id: TrackId,
        kind: TrackKind,
        selected_tracks: &[TrackId],
    ) {
        if ui.button("Group selected tracks").clicked() {
            let tracks = if selected_tracks.is_empty() {
                vec![track_id]
            } else {
                selected_tracks.to_vec()
            };
            self.emit(UiIntent::GroupTracks { tracks });
            ui.close();
        }
        if ui.button("Add audio track").clicked() {
            self.emit(UiIntent::CreateTrack(TrackKind::Audio));
            ui.close();
        }
        if ui.button("Add instrument track").clicked() {
            self.emit(UiIntent::CreateTrack(TrackKind::Instrument));
            ui.close();
        }
        if ui.button("Add group track").clicked() {
            self.emit(UiIntent::CreateTrack(TrackKind::Group));
            ui.close();
        }
        if kind != TrackKind::Master && ui.button("Delete track").clicked() {
            self.emit(UiIntent::DeleteTrack(track_id));
            ui.close();
        }
    }

    pub(super) fn clip_context_menu(
        &mut self,
        ui: &mut egui::Ui,
        clip: &notwig_core::ClipEntity,
        runtime: &RuntimeView,
    ) {
        if ui
            .button(if clip.muted { "Unmute" } else { "Mute" })
            .clicked()
        {
            self.emit(UiIntent::SetClipMuted {
                clip_id: clip.id,
                muted: !clip.muted,
            });
            ui.close();
        }
        if ui.button("Duplicate").clicked() {
            self.emit(UiIntent::DuplicateClip { clip_id: clip.id });
            ui.close();
        }
        let at = runtime.playhead;
        if at > clip.range.start && at < clip.range.end && ui.button("Split at playhead").clicked()
        {
            self.emit(UiIntent::SplitClip {
                clip_id: clip.id,
                at,
            });
            ui.close();
        }
        if ui.button("Delete").clicked() {
            self.emit(UiIntent::DeleteClip(clip.id));
            ui.close();
        }
    }
}
