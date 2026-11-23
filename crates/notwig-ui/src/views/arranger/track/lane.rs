use egui::{Color32, CornerRadius, Id, Rect, Sense, Stroke, StrokeKind};
use notwig_core::tracks::Track;
use notwig_core::{ClipId, ProjectDoc, RangeMus, TimeMus, TrackId, TrackKind};

use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::shell::NotwigUi;
use crate::theme;
use crate::timeline::TimelineGrid;
use crate::widgets::{SnapSettings, beats_to_time, snap_time};

use super::super::TimeDrag;

impl NotwigUi {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn interact_track_lane(
        &mut self,
        ui: &mut egui::Ui,
        doc: &ProjectDoc,
        track: &Track,
        track_id: TrackId,
        lane_rect: Rect,
        start: TimeMus,
        ppb: f32,
        grid: TimeMus,
        snap: SnapSettings,
        runtime: &RuntimeView,
    ) -> Vec<ClipId> {
        let lane_resp = ui.interact(
            lane_rect,
            Id::new(("lane", track_id)),
            Sense::click_and_drag(),
        );
        let lane_double_click = ui.input(|input| {
            input
                .pointer
                .button_double_clicked(egui::PointerButton::Primary)
                .then(|| input.pointer.interact_pos())
                .flatten()
                .filter(|pos| lane_rect.contains(*pos))
        });
        if let Some(pos) = lane_double_click {
            self.lane_double_click(
                doc,
                track_id,
                pos,
                lane_rect,
                start,
                ppb,
                grid,
                snap.bypassed(ui.input(|i| i.modifiers)),
            );
        }
        if lane_resp.clicked() {
            if let Some(pos) = lane_resp.interact_pointer_pos() {
                let shift = snap.bypassed(ui.input(|i| i.modifiers));
                let beat = snap_time(
                    self.state.viewport.x_to_beat(pos.x, lane_rect.left()),
                    grid,
                    shift,
                );
                if let Ok(range) = RangeMus::new(beat, beat) {
                    self.state.selection.select_time(range, &[track_id]);
                }
            }
        }
        if lane_resp.drag_started_by(egui::PointerButton::Primary) {
            if let Some(pos) = ui
                .input(|i| i.pointer.press_origin())
                .or_else(|| lane_resp.interact_pointer_pos())
            {
                let shift = snap.bypassed(ui.input(|i| i.modifiers));
                let beat = snap_time(
                    self.state.viewport.x_to_beat(pos.x, lane_rect.left()),
                    grid,
                    shift,
                );
                let range = RangeMus::new(beat, beat).unwrap();
                self.state.selection.select_time(range, &[track_id]);
                self.time_drag = Some(TimeDrag {
                    start_beat: beat,
                    start_track: track_id,
                    start_y: pos.y,
                });
            }
        }
        if lane_resp.dragged() {
            if let Some(drag) = self.time_drag {
                if let Some(pos) = lane_resp.interact_pointer_pos() {
                    let shift = snap.bypassed(ui.input(|i| i.modifiers));
                    let beat = snap_time(
                        self.state.viewport.x_to_beat(pos.x, lane_rect.left()),
                        grid,
                        shift,
                    );
                    if let Some(range) =
                        crate::timeline::interaction::selection_range(drag.start_beat, beat)
                    {
                        let min_y = drag.start_y.min(pos.y);
                        let max_y = drag.start_y.max(pos.y);
                        let mut tracks = Vec::new();
                        for id in doc.track_order.depth_first() {
                            if let Some(row) = self.row_rect(id) {
                                if row.bottom() >= min_y && row.top() <= max_y {
                                    tracks.push(id);
                                }
                            }
                        }
                        if tracks.is_empty() {
                            tracks.push(drag.start_track);
                        }
                        self.state.selection.select_time(range, &tracks);
                    }
                }
            }
        }
        if lane_resp.drag_stopped()
            && self.time_drag.take().is_some()
            && self
                .state
                .selection
                .time
                .as_ref()
                .is_some_and(|time| time.range.is_empty())
        {
            self.state.selection.time = None;
        }
        let clip_ids = doc
            .arranger
            .lanes
            .get(&track_id)
            .map(|lane| lane.clips.clone())
            .unwrap_or_default();
        let secondary_pos = ui.input(|input| {
            input
                .pointer
                .button_clicked(egui::PointerButton::Secondary)
                .then(|| input.pointer.interact_pos())
                .flatten()
                .filter(|pos| lane_rect.contains(*pos))
        });
        let clip_under_pointer = secondary_pos.and_then(|pos| {
            let beat = self.state.viewport.x_to_beat(pos.x, lane_rect.left());
            Self::clip_at(doc, track_id, beat, &clip_ids)
        });
        let menu_clip_state = Id::new(("lane-context-clip", track_id));
        if let Some(clip_id) = clip_under_pointer {
            ui.ctx()
                .memory_mut(|memory| memory.data.insert_temp(menu_clip_state, Some(clip_id)));
            self.state.selection.select_clip(clip_id, track_id, false);
            self.emit(UiIntent::SelectClip(clip_id));
        } else if secondary_pos.is_some() {
            ui.ctx()
                .memory_mut(|memory| memory.data.insert_temp(menu_clip_state, None::<ClipId>));
        }
        let menu_clip = ui
            .ctx()
            .memory(|memory| memory.data.get_temp::<Option<ClipId>>(menu_clip_state))
            .flatten();
        lane_resp.context_menu(|ui| {
            if let Some(clip_id) = menu_clip {
                if let Some(clip) = doc.arranger.clips.get(&clip_id) {
                    self.clip_context_menu(ui, clip, runtime);
                }
            } else if track.kind != TrackKind::Master {
                if ui.button("Add MIDI clip at playhead").clicked() {
                    if let Ok(range) = RangeMus::new(
                        doc.transport.playhead,
                        doc.transport.playhead + TimeMus::new(4, 1),
                    ) {
                        self.emit(UiIntent::CreateMidiClip { track_id, range });
                    }
                    ui.close();
                }
                if track.kind == TrackKind::Audio {
                    ui.menu_button("Insert audio asset", |ui| {
                        let assets: Vec<_> = doc
                            .assets
                            .assets
                            .values()
                            .filter(|asset| asset.kind == notwig_core::AssetKind::Audio)
                            .collect();
                        if assets.is_empty() {
                            ui.label("No audio assets imported");
                        }
                        for asset in assets {
                            if ui.button(&asset.uri).clicked() {
                                self.emit(UiIntent::InsertAudioClip {
                                    track_id,
                                    asset_id: asset.id,
                                });
                                ui.close();
                            }
                        }
                    });
                }
            }
        });
        clip_ids
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_track_lane(
        &mut self,
        ui: &mut egui::Ui,
        doc: &ProjectDoc,
        painter: &egui::Painter,
        track: &Track,
        track_id: TrackId,
        clip_ids: &[ClipId],
        header_rect: Rect,
        lane_rect: Rect,
        row_rect: Rect,
        start: TimeMus,
        ppb: f32,
        runtime: &RuntimeView,
        grid: TimeMus,
        snap: SnapSettings,
    ) {
        for clip_id in clip_ids {
            self.draw_clip(
                ui, doc, track_id, *clip_id, lane_rect, start, ppb, runtime, grid,
            );
        }
        for take in runtime
            .recording_takes
            .iter()
            .filter(|take| take.track_id == track_id)
        {
            self.draw_recording_take(ui, doc, take, lane_rect, start, ppb);
        }
        if let Some((preview_clip, preview_range, preview_track)) = self.clip_preview {
            if preview_track == track_id
                && doc
                    .arranger
                    .clips
                    .get(&preview_clip)
                    .is_some_and(|clip| clip.track_id != track_id)
            {
                let x1 =
                    lane_rect.left() + (preview_range.start.as_f64() - start.as_f64()) as f32 * ppb;
                let x2 =
                    lane_rect.left() + (preview_range.end.as_f64() - start.as_f64()) as f32 * ppb;
                painter.rect_stroke(
                    Rect::from_min_max(
                        egui::pos2(x1, lane_rect.top() + 3.0),
                        egui::pos2(x2, lane_rect.bottom() - 3.0),
                    ),
                    CornerRadius::same(3),
                    Stroke::new(1.5_f32, theme::ACCENT),
                    StrokeKind::Inside,
                );
            }
        }

        if let Some(time) = &self.state.selection.time
            && time.contains_track(track_id)
        {
            crate::timeline::draw_time_selection_fill(
                painter,
                TimelineGrid::new(
                    start,
                    ppb,
                    doc.meta.bar_length(),
                    doc.meta.beat_length(),
                    snap,
                ),
                lane_rect.left(),
                lane_rect,
                time.range,
            );
        }

        if !track.active {
            let dim = Color32::from_black_alpha(110);
            painter.rect_filled(header_rect, CornerRadius::ZERO, dim);
            painter.rect_filled(lane_rect, CornerRadius::ZERO, dim);
        }
        if track.kind == TrackKind::Master {
            painter.line_segment(
                [
                    egui::pos2(lane_rect.left(), row_rect.top()),
                    egui::pos2(lane_rect.right(), row_rect.top()),
                ],
                Stroke::new(1.0_f32, theme::GRID),
            );
        }
        painter.line_segment(
            [
                egui::pos2(lane_rect.left(), row_rect.bottom()),
                egui::pos2(lane_rect.right(), row_rect.bottom()),
            ],
            Stroke::new(1.0_f32, theme::GRID),
        );
    }

    fn clip_at(
        doc: &ProjectDoc,
        track_id: TrackId,
        beat: TimeMus,
        clip_ids: &[ClipId],
    ) -> Option<ClipId> {
        clip_ids.iter().copied().find(|clip_id| {
            doc.arranger
                .clips
                .get(clip_id)
                .is_some_and(|clip| clip.track_id == track_id && clip.range.contains(beat))
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn lane_double_click(
        &mut self,
        doc: &ProjectDoc,
        track_id: TrackId,
        pos: egui::Pos2,
        lane_rect: Rect,
        start: TimeMus,
        ppb: f32,
        grid: TimeMus,
        bypass_snap: bool,
    ) {
        let Some(track) = doc.tracks.get(&track_id) else {
            return;
        };
        let alt = false;
        let beat = start.as_f64() + f64::from((pos.x - lane_rect.left()) / ppb);
        let beat = snap_time(beats_to_time(beat), grid, bypass_snap || alt);
        let clip_ids = doc
            .arranger
            .lanes
            .get(&track_id)
            .map(|lane| lane.clips.clone())
            .unwrap_or_default();
        if clip_ids.iter().any(|clip_id| {
            doc.arranger
                .clips
                .get(clip_id)
                .is_some_and(|clip| clip.range.contains(beat))
        }) {
            return;
        }
        let mut end = beat + TimeMus::new(4, 1);
        for _ in 0..clip_ids.len() {
            let Some(blocking) = clip_ids
                .iter()
                .filter_map(|clip_id| doc.arranger.clips.get(clip_id))
                .find(|clip| clip.range.contains(end))
            else {
                break;
            };
            if blocking.range.start <= beat {
                return;
            }
            if blocking.range.start == end {
                break;
            }
            end = blocking.range.start;
        }
        if end == beat {
            return;
        }
        match track.kind {
            TrackKind::Instrument => {
                if let Ok(range) = RangeMus::new(beat, end) {
                    self.emit(UiIntent::CreateMidiClip { track_id, range });
                }
            }
            TrackKind::Audio => {
                if let Ok(range) = RangeMus::new(beat, end) {
                    self.emit(UiIntent::CreateAudioClip { track_id, range });
                }
            }
            _ => {}
        }
    }
}
