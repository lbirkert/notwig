use egui::{Color32, CornerRadius, Id, Rect, Sense, Stroke, StrokeKind, Vec2};
use notwig_core::edit::Edge;
use notwig_core::sequencer::{AudioClipData, ClipPayload};
use notwig_core::{
    AssetId, ClipEntity, ClipId, ClipKind, Db, ProjectDoc, RangeMus, TimeMus, TrackId, TrackKind,
};

use crate::intent::UiIntent;
use crate::runtime::{RecordingTakeView, RuntimeView};
use crate::shell::NotwigUi;
use crate::timeline::TimelineGrid;
use crate::timeline::interaction::{drag_current_pos, drag_start_pos, snapped_start, trim_range};
use crate::waveform::WaveformCache;

use super::edit::clamp_node_shift;
use super::{ClipDrag, ClipDragMode, GroupDrag};

impl NotwigUi {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_recording_take(
        &self,
        ui: &mut egui::Ui,
        doc: &ProjectDoc,
        take: &RecordingTakeView,
        lane_rect: Rect,
        start: TimeMus,
        ppb: f32,
    ) {
        let Ok(range) = RangeMus::new(take.start, take.end) else {
            return;
        };
        let asset_id = AssetId::from_uuid(*take.track_id.as_uuid());
        let source_end = doc.tempo_map().mus_to_samples(range.length());
        let mut waveforms = WaveformCache::new();
        waveforms.insert(asset_id, (*take.waveform).clone());
        let clip = ClipEntity {
            id: ClipId::from_uuid(*take.track_id.as_uuid()),
            track_id: take.track_id,
            kind: ClipKind::Audio,
            range,
            name: Some("Recording".into()),
            color: None,
            muted: false,
            payload: ClipPayload::Audio(AudioClipData {
                asset_id,
                source_start_sample: 0,
                source_end_sample: source_end,
                gain_db: Db(0.0),
                gain_curve: None,
                pan_curve: None,
                fade_in_samples: 0,
                fade_out_samples: 0,
                fade_shape: notwig_core::FadeShape::Linear,
                slices: Vec::new(),
                slices_enabled: false,
            }),
        };
        let _ = crate::timeline::draw_clip_visual(
            ui.painter(),
            doc,
            &clip,
            range,
            lane_rect,
            TimelineGrid::new(
                start,
                ppb,
                doc.meta.bar_length(),
                doc.meta.beat_length(),
                self.state.snap,
            ),
            &waveforms,
            false,
            true,
        );
    }

    pub(super) fn draw_clip(
        &mut self,
        ui: &mut egui::Ui,
        doc: &ProjectDoc,
        track_id: TrackId,
        clip_id: ClipId,
        lane_rect: Rect,
        start: TimeMus,
        ppb: f32,
        runtime: &RuntimeView,
        grid: TimeMus,
    ) {
        let Some(clip) = doc.arranger.clips.get(&clip_id) else {
            return;
        };
        let snap = self.state.snap;
        let preview = self
            .group_preview
            .iter()
            .find(|(id, _, _)| *id == clip_id)
            .map(|(_, _, range)| *range)
            .or_else(|| {
                self.clip_preview
                    .filter(|(id, _, _)| *id == clip_id)
                    .map(|(_, range, _)| range)
            });
        let range = preview.unwrap_or(clip.range);
        let selected = self.state.selection.clips.contains(&clip_id);
        let Some(visual) = crate::timeline::draw_clip_visual(
            ui.painter(),
            doc,
            clip,
            range,
            lane_rect,
            TimelineGrid::new(
                start,
                ppb,
                doc.meta.bar_length(),
                doc.meta.beat_length(),
                snap,
            ),
            &runtime.waveforms,
            selected,
            preview.is_some(),
        ) else {
            return;
        };
        let clip_rect = visual.clip_rect;
        let name_rect = visual.name_rect;
        let painter = ui.painter().with_clip_rect(lane_rect);

        let name_visible = name_rect.intersect(lane_rect);
        let name_response = ui.interact(
            name_visible,
            Id::new(("clip-name", clip_id)),
            Sense::click_and_drag(),
        );
        if name_response.clicked() {
            let additive = ui.input(|i| i.modifiers.command || i.modifiers.shift);
            self.state
                .selection
                .select_clip(clip_id, track_id, additive);
            self.emit(UiIntent::SelectClip(clip_id));
        }
        if name_response.double_clicked() {
            self.open_editor_for_clip(doc, clip_id);
        }
        name_response.context_menu(|ui| {
            if ui
                .button(if clip.muted { "Unmute" } else { "Mute" })
                .clicked()
            {
                self.emit(UiIntent::SetClipMuted {
                    clip_id,
                    muted: !clip.muted,
                });
                ui.close();
            }
            if ui.button("Duplicate").clicked() {
                self.emit(UiIntent::DuplicateClip { clip_id });
                ui.close();
            }
            let at = runtime.playhead;
            if at > clip.range.start
                && at < clip.range.end
                && ui.button("Split at playhead").clicked()
            {
                self.emit(UiIntent::SplitClip { clip_id, at });
                ui.close();
            }
            if ui.button("Delete").clicked() {
                self.emit(UiIntent::DeleteClip(clip_id));
                ui.close();
            }
        });
        if name_response.drag_started() {
            if let Some(pos) = drag_start_pos(ui, &name_response) {
                if self.state.selection.clips.len() > 1
                    && self.state.selection.clips.contains(&clip_id)
                {
                    self.begin_group_drag(clip, pos);
                } else {
                    self.begin_clip_drag(clip, clip_id, track_id, ClipDragMode::Move, pos);
                }
            }
        }
        if self.group_drag.is_some() {
            self.update_group_drag(ui, &name_response, ppb, grid);
        } else {
            self.update_clip_drag(ui, doc, &name_response, clip, clip_id, lane_rect, ppb, grid);
        }

        for (mode, edge_x, id) in [
            (ClipDragMode::TrimStart, clip_rect.left(), "trim-start"),
            (ClipDragMode::TrimEnd, clip_rect.right(), "trim-end"),
        ] {
            let handle = Rect::from_min_max(
                egui::pos2(edge_x - 4.0, name_rect.top()),
                egui::pos2(edge_x + 4.0, name_rect.bottom()),
            )
            .intersect(lane_rect);
            if handle.width() < 1.0 {
                continue;
            }
            let response = ui.interact(handle, Id::new(("clip", clip_id, id)), Sense::drag());
            if response.hovered() || response.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                let edge_grip = Rect::from_center_size(
                    egui::pos2(edge_x, name_rect.center().y),
                    Vec2::new(4.0, (name_rect.height() - 4.0).max(8.0)),
                )
                .intersect(lane_rect);
                painter.rect_filled(edge_grip, CornerRadius::same(2), Color32::WHITE);
                painter.rect_stroke(
                    edge_grip,
                    CornerRadius::same(2),
                    Stroke::new(1.0_f32, Color32::from_black_alpha(160)),
                    StrokeKind::Inside,
                );
            }
            if response.drag_started() {
                if let Some(pos) = drag_start_pos(ui, &response) {
                    self.begin_clip_drag(clip, clip_id, track_id, mode, pos);
                }
            }
            self.update_clip_drag(ui, doc, &response, clip, clip_id, lane_rect, ppb, grid);
        }
    }

    fn begin_clip_drag(
        &mut self,
        clip: &notwig_core::ClipEntity,
        clip_id: ClipId,
        track_id: TrackId,
        mode: ClipDragMode,
        pointer: egui::Pos2,
    ) {
        self.drag = Some(ClipDrag {
            clip_id,
            track_id,
            mode,
            origin: clip.range,
            start_pointer: pointer,
        });
        if self.state.selection.clips.contains(&clip_id) {
            self.state.selection.clips.set_primary(clip_id);
        } else {
            self.state.selection.select_clip(clip_id, track_id, false);
        }
    }

    fn begin_group_drag(&mut self, clip: &notwig_core::ClipEntity, pointer: egui::Pos2) {
        let origins: Vec<(ClipId, TrackId, RangeMus)> = self
            .state
            .selection
            .clips
            .iter()
            .filter_map(|id| self.doc_clip_origin(*id))
            .collect();
        if origins.is_empty() {
            return;
        }
        self.group_drag = Some(GroupDrag {
            start_pointer: pointer,
            primary_origin: clip.range,
            origins,
        });
    }

    fn doc_clip_origin(&self, id: ClipId) -> Option<(ClipId, TrackId, RangeMus)> {
        self.clip_origins.get(&id).copied()
    }

    fn update_group_drag(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        ppb: f32,
        grid: TimeMus,
    ) {
        let snap = self.state.snap;
        if response.dragged() {
            if let Some(drag) = self.group_drag.clone() {
                let current = drag_current_pos(ui, response, drag.start_pointer);
                let bypass = snap.bypassed(ui.input(|i| i.modifiers));
                let delta = f64::from(current.x - drag.start_pointer.x) / f64::from(ppb);
                let primary_start = snapped_start(drag.primary_origin.start, delta, grid, bypass);
                let starts: Vec<TimeMus> = drag
                    .origins
                    .iter()
                    .map(|(_, _, range)| range.start)
                    .collect();
                let shift =
                    clamp_node_shift(primary_start - drag.primary_origin.start, &starts, None);
                self.group_preview = drag
                    .origins
                    .iter()
                    .map(|(id, track, origin)| {
                        let start = origin.start + shift;
                        let range =
                            RangeMus::new(start, start + origin.length()).unwrap_or(*origin);
                        (*id, *track, range)
                    })
                    .collect();
            }
        }
        if response.drag_stopped() && self.group_drag.take().is_some() {
            let previews = std::mem::take(&mut self.group_preview);
            let moves: Vec<_> = previews
                .into_iter()
                .filter(|(id, _, range)| {
                    self.clip_origins
                        .get(id)
                        .is_some_and(|(_, _, origin)| origin != range)
                })
                .collect();
            if !moves.is_empty() {
                self.emit(UiIntent::MoveClips { clips: moves });
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn update_clip_drag(
        &mut self,
        ui: &egui::Ui,
        doc: &ProjectDoc,
        response: &egui::Response,
        clip: &notwig_core::ClipEntity,
        clip_id: ClipId,
        _lane_rect: Rect,
        ppb: f32,
        grid: TimeMus,
    ) {
        let snap = self.state.snap;
        if response.dragged() {
            if let Some(drag) = self.drag.filter(|drag| drag.clip_id == clip_id) {
                let current = drag_current_pos(ui, response, drag.start_pointer);
                let bypass = snap.bypassed(ui.input(|i| i.modifiers));
                let delta = f64::from(current.x - drag.start_pointer.x) / f64::from(ppb);
                let range = drag_range(drag, delta, bypass, grid);
                let target = if drag.mode == ClipDragMode::Move {
                    ui.input(|i| i.pointer.hover_pos())
                        .and_then(|pos| self.track_at(pos.y))
                        .filter(|target| compatible_track(doc, clip.kind, *target))
                        .unwrap_or(drag.track_id)
                } else {
                    drag.track_id
                };
                self.clip_preview = Some((clip_id, range, target));
            }
        }
        if response.drag_stopped() {
            if let Some((id, range, target_track)) = self.clip_preview.take() {
                if let Some(drag) = self.drag.take() {
                    if range != drag.origin || target_track != drag.track_id {
                        if drag.mode == ClipDragMode::Move {
                            self.emit(UiIntent::MoveClip {
                                clip_id: id,
                                track_id: target_track,
                                range,
                            });
                        } else if drag.mode == ClipDragMode::TrimStart
                            && clip.kind == ClipKind::Audio
                            && range.start < drag.origin.start
                        {
                            self.emit(UiIntent::SetClipRange { clip_id: id, range });
                        } else {
                            self.emit(UiIntent::EditClipRange { clip_id: id, range });
                        }
                    }
                }
            } else {
                self.drag = None;
            }
        }
    }
}

fn compatible_track(doc: &ProjectDoc, kind: ClipKind, track_id: TrackId) -> bool {
    doc.tracks.get(&track_id).is_some_and(|track| match kind {
        ClipKind::Midi => track.kind == TrackKind::Instrument,
        ClipKind::Audio => track.kind == TrackKind::Audio,
    })
}
fn drag_range(drag: ClipDrag, delta: f64, bypass: bool, grid: TimeMus) -> RangeMus {
    let origin_end = drag.origin.end.as_f64();
    let length = (origin_end - drag.origin.start.as_f64()).max(1.0 / 16.0);
    match drag.mode {
        ClipDragMode::Move => {
            let start = snapped_start(drag.origin.start, delta, grid, bypass);
            RangeMus::new(
                start,
                start + TimeMus::new((length * 1_000_000.0) as i64, 1_000_000),
            )
            .unwrap_or(drag.origin)
        }
        ClipDragMode::TrimStart => trim_range(
            drag.origin,
            Edge::Start,
            delta,
            grid,
            bypass,
            TimeMus::new(1, 16),
        ),
        ClipDragMode::TrimEnd => trim_range(
            drag.origin,
            Edge::End,
            delta,
            grid,
            bypass,
            TimeMus::new(1, 16),
        ),
    }
}
