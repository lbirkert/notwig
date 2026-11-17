use egui::{Align2, Color32, CornerRadius, FontId, Id, Rect, Sense, Stroke, StrokeKind, Vec2};
use notwig_core::{
    ClipId, ClipKind, ClipPayload, ProjectDoc, RangeMus, TimeMus, TrackId, TrackKind,
};

use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::shell::{NotwigUi, clip_color, track_depth};
use crate::theme;
use crate::widgets::{format_db, snap_step, snap_time};

const HEADER_W: f32 = 230.0;
const ROW_H: f32 = 48.0;
const RULER_H: f32 = 30.0;
const SCROLLBAR_H: f32 = 14.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipDragMode {
    Move,
    TrimStart,
    TrimEnd,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClipDrag {
    pub clip_id: ClipId,
    pub track_id: TrackId,
    pub mode: ClipDragMode,
    pub origin: RangeMus,
    pub start_pointer: egui::Pos2,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimeDrag {
    pub start_beat: TimeMus,
    pub start_track: TrackId,
    pub start_y: f32,
}

impl NotwigUi {
    pub(crate) fn arranger_view(
        &mut self,
        ui: &mut egui::Ui,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
    ) {
        let ppb = self.state.viewport.pixels_per_beat.clamp(
            crate::state::Viewport::MIN_PPB,
            crate::state::Viewport::MAX_PPB,
        );
        let start = self.state.viewport.start;
        let beats_per_bar = doc.meta.time_sig_num.max(1);
        let grid = snap_step(ppb, beats_per_bar);
        let width = ui.available_width().max(320.0);
        let header_w = HEADER_W.min(width * 0.45);
        self.timeline_width = (width - header_w).max(80.0);
        self.arranger_rect = ui.max_rect();
        self.timeline_left = self.arranger_rect.left() + header_w;
        self.handle_navigation(ui);

        let (ruler_rect, ruler_resp) =
            ui.allocate_exact_size(Vec2::new(width, RULER_H), Sense::click_and_drag());
        let timeline_left = ruler_rect.left() + header_w;
        self.timeline_left = timeline_left;
        self.draw_ruler(
            ui,
            ruler_rect,
            &ruler_resp,
            doc,
            runtime,
            header_w,
            timeline_left,
            ppb,
            beats_per_bar,
            grid,
        );

        let rows_top = ui.cursor().top();
        let rows_height = (ui.available_height() - SCROLLBAR_H - 4.0).max(60.0);
        ui.allocate_ui_with_layout(
            Vec2::new(width, rows_height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("arranger_rows")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for track_id in doc.track_order.depth_first() {
                            self.track_row(ui, doc, track_id, header_w, ppb, start, runtime, grid);
                        }
                        let (add_rect, add_resp) =
                            ui.allocate_exact_size(Vec2::new(width, 28.0), Sense::click());
                        let painter = ui.painter();
                        painter.rect_filled(add_rect, CornerRadius::ZERO, theme::PANEL);
                        painter.text(
                            egui::pos2(add_rect.left() + 14.0, add_rect.center().y),
                            Align2::LEFT_CENTER,
                            "+ Add track",
                            FontId::proportional(11.0),
                            theme::TEXT_MUTED,
                        );
                        if add_resp.clicked() {
                            self.emit(UiIntent::CreateTrack(TrackKind::Audio));
                        }
                    });
            },
        );

        let content_rect = Rect::from_min_max(
            egui::pos2(timeline_left, rows_top),
            egui::pos2(ruler_rect.right(), rows_top + rows_height),
        );
        let overlay = ui.painter().with_clip_rect(content_rect);
        let playhead_x = timeline_left + (runtime.playhead.as_f64() - start.as_f64()) as f32 * ppb;
        if playhead_x >= content_rect.left() && playhead_x <= content_rect.right() {
            overlay.line_segment(
                [
                    egui::pos2(playhead_x, content_rect.top()),
                    egui::pos2(playhead_x, content_rect.bottom()),
                ],
                Stroke::new(1.0_f32, Color32::WHITE),
            );
        }
        if let Some(time) = &self.state.selection.time {
            for beat in [time.range.start, time.range.end] {
                let x = timeline_left + (beat.as_f64() - start.as_f64()) as f32 * ppb;
                if x >= content_rect.left() && x <= content_rect.right() {
                    overlay.line_segment(
                        [
                            egui::pos2(x, content_rect.top()),
                            egui::pos2(x, content_rect.bottom()),
                        ],
                        Stroke::new(1.0_f32, Color32::WHITE),
                    );
                }
            }
        }
        let (bar_rect, _) = ui.allocate_exact_size(Vec2::new(width, SCROLLBAR_H), Sense::hover());
        self.draw_drop_preview(ui, doc, header_w);
        self.timeline_scrollbar(ui, bar_rect, doc, header_w, ppb);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_ruler(
        &mut self,
        ui: &mut egui::Ui,
        ruler_rect: Rect,
        ruler_resp: &egui::Response,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
        header_w: f32,
        timeline_left: f32,
        ppb: f32,
        beats_per_bar: u8,
        grid: TimeMus,
    ) {
        let painter = ui.painter().clone();
        painter.rect_filled(ruler_rect, CornerRadius::ZERO, theme::PANEL_LIGHT);
        painter.line_segment(
            [
                egui::pos2(ruler_rect.left() + header_w, ruler_rect.top()),
                egui::pos2(ruler_rect.left() + header_w, ruler_rect.bottom()),
            ],
            Stroke::new(1.0_f32, theme::GRID_STRONG),
        );
        painter.text(
            egui::pos2(ruler_rect.left() + 10.0, ruler_rect.center().y),
            Align2::LEFT_CENTER,
            "TRACKS",
            FontId::proportional(11.0),
            theme::TEXT_MUTED,
        );
        let start = self.state.viewport.start;
        let visible_beats = f64::from(((ruler_rect.right() - timeline_left) / ppb).max(1.0));
        if doc.transport.loop_enabled {
            if let Some(range) = doc.transport.loop_range {
                let x1 = timeline_left + (range.start.as_f64() - start.as_f64()) as f32 * ppb;
                let x2 = timeline_left + (range.end.as_f64() - start.as_f64()) as f32 * ppb;
                painter.rect_filled(
                    Rect::from_min_max(
                        egui::pos2(x1.max(timeline_left), ruler_rect.top()),
                        egui::pos2(x2.min(ruler_rect.right()), ruler_rect.bottom()),
                    ),
                    CornerRadius::ZERO,
                    theme::ACCENT.gamma_multiply(0.18),
                );
            }
        }
        if let Some(time) = &self.state.selection.time {
            let x1 = timeline_left + (time.range.start.as_f64() - start.as_f64()) as f32 * ppb;
            let x2 = timeline_left + (time.range.end.as_f64() - start.as_f64()) as f32 * ppb;
            painter.rect_filled(
                Rect::from_min_max(
                    egui::pos2(x1.max(timeline_left), ruler_rect.top()),
                    egui::pos2(x2.min(ruler_rect.right()), ruler_rect.bottom()),
                ),
                CornerRadius::ZERO,
                Color32::from_rgba_unmultiplied(255, 210, 120, 34),
            );
        }
        let step = grid.as_f64().max(1.0 / 256.0);
        let bar_len = f64::from(beats_per_bar);
        let show_beats = ppb >= 26.0;
        let first_index = (start.as_f64() / step).floor() as i64;
        let last_index = ((start.as_f64() + visible_beats) / step).ceil() as i64 + 1;
        for index in first_index..=last_index {
            let beat = index as f64 * step;
            if beat < 0.0 {
                continue;
            }
            let x = timeline_left + (beat - start.as_f64()) as f32 * ppb;
            if x < timeline_left - 1.0 || x > ruler_rect.right() + 1.0 {
                continue;
            }
            let strong = (beat / bar_len).fract().abs() < 1e-6;
            let beat_line = (beat.fract()).abs() < 1e-6;
            let label = if strong || (show_beats && beat_line) {
                let bar = (beat / bar_len).floor() as i64 + 1;
                let beat_in_bar = (beat % bar_len).floor() as i64 + 1;
                format!("{bar}.{beat_in_bar}")
            } else {
                String::new()
            };
            painter.line_segment(
                [
                    egui::pos2(
                        x,
                        ruler_rect.top()
                            + if strong {
                                6.0
                            } else if beat_line {
                                14.0
                            } else {
                                20.0
                            },
                    ),
                    egui::pos2(x, ruler_rect.bottom()),
                ],
                Stroke::new(
                    1.0_f32,
                    if strong {
                        theme::GRID_STRONG
                    } else if beat_line {
                        theme::GRID
                    } else {
                        theme::GRID.gamma_multiply(0.55)
                    },
                ),
            );
            if !label.is_empty() {
                painter.text(
                    egui::pos2(x + 3.0, ruler_rect.top() + 3.0),
                    Align2::LEFT_TOP,
                    label,
                    FontId::monospace(9.0),
                    if strong {
                        theme::TEXT_MUTED
                    } else {
                        theme::GRID_STRONG
                    },
                );
            }
        }
        let play_start_x =
            timeline_left + (runtime.play_start.as_f64() - start.as_f64()) as f32 * ppb;
        if play_start_x >= timeline_left && play_start_x <= ruler_rect.right() {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(play_start_x, ruler_rect.top() + 4.0),
                    egui::pos2(play_start_x, ruler_rect.top() + 14.0),
                    egui::pos2(play_start_x + 8.0, ruler_rect.top() + 9.0),
                ],
                theme::ACCENT,
                Stroke::NONE,
            ));
        }
        if (ruler_resp.clicked() || ruler_resp.dragged())
            && ruler_resp.interact_pointer_pos().is_some()
        {
            if let Some(pos) = ruler_resp.interact_pointer_pos() {
                if pos.x > timeline_left {
                    let beat = start.as_f64() + f64::from((pos.x - timeline_left) / ppb);
                    let shift = ui.input(|i| i.modifiers.shift);
                    let beat = TimeMus::new((beat * 1_000_000.0).round() as i64, 1_000_000);
                    self.emit(UiIntent::SetPlayStart(snap_time(beat, grid, shift)));
                }
            }
        }
    }

    fn handle_navigation(&mut self, ui: &mut egui::Ui) {
        let Some(pos) = ui.input(|i| i.pointer.hover_pos()) else {
            return;
        };
        if !self.arranger_rect.contains(pos) {
            return;
        }
        let ppb = self.state.viewport.pixels_per_beat.clamp(
            crate::state::Viewport::MIN_PPB,
            crate::state::Viewport::MAX_PPB,
        );
        let zoom_delta = ui.input(egui::InputState::zoom_delta);
        if (zoom_delta - 1.0).abs() > 1e-4 {
            self.zoom_at(pos.x, zoom_delta);
        }
        let middle = ui.input(|i| i.pointer.middle_down());
        if middle {
            let delta = ui.input(|i| i.pointer.delta());
            if delta.x.abs() > 0.0 {
                self.pan_beats(-f64::from(delta.x) / f64::from(ppb));
            }
            if delta.y.abs() > 0.0 {
                self.zoom_at(pos.x, (-f64::from(delta.y) * 0.012).exp() as f32);
            }
        }
        let scroll = ui.input(|i| i.smooth_scroll_delta);
        if !middle && scroll.x.abs() > scroll.y.abs() && scroll.x.abs() > 0.01 {
            self.pan_beats(-f64::from(scroll.x) / f64::from(ppb));
        }
    }

    fn zoom_at(&mut self, pointer_x: f32, factor: f32) {
        let ppb = self.state.viewport.pixels_per_beat.clamp(
            crate::state::Viewport::MIN_PPB,
            crate::state::Viewport::MAX_PPB,
        );
        let start = self.state.viewport.start.as_f64();
        let beat_under = start + f64::from((pointer_x - self.timeline_left) / ppb);
        let new_ppb = (ppb * factor).clamp(4.0, 800.0);
        let new_start =
            (beat_under - f64::from((pointer_x - self.timeline_left) / new_ppb)).max(0.0);
        self.state.viewport.pixels_per_beat = new_ppb;
        self.state.viewport.start =
            TimeMus::new((new_start * 1_000_000.0).round() as i64, 1_000_000);
    }

    fn pan_beats(&mut self, delta: f64) {
        let start = (self.state.viewport.start.as_f64() + delta).max(0.0);
        self.state.viewport.start = TimeMus::new((start * 1_000_000.0).round() as i64, 1_000_000);
    }

    #[allow(clippy::too_many_arguments)]
    fn track_row(
        &mut self,
        ui: &mut egui::Ui,
        doc: &ProjectDoc,
        track_id: TrackId,
        header_w: f32,
        ppb: f32,
        start: TimeMus,
        runtime: &RuntimeView,
        grid: TimeMus,
    ) {
        let Some(track) = doc.tracks.get(&track_id) else {
            return;
        };
        let width = ui.available_width().max(320.0);
        let (row_rect, _) = ui.allocate_exact_size(Vec2::new(width, ROW_H), Sense::hover());
        self.track_rows.insert(track_id, row_rect);
        let header_rect = Rect::from_min_size(
            row_rect.min,
            Vec2::new(header_w.min(row_rect.width()), row_rect.height()),
        );
        let lane_rect = Rect::from_min_max(
            egui::pos2(header_rect.right(), row_rect.top()),
            row_rect.max,
        );
        let selected_track = self.state.selection.tracks.contains(&track_id)
            || self.state.selection.tracks.primary() == Some(track_id);
        let painter = ui.painter().clone();
        painter.rect_filled(
            header_rect,
            CornerRadius::ZERO,
            if selected_track {
                theme::PANEL
            } else {
                theme::PANEL_LIGHT
            },
        );
        painter.rect_filled(
            lane_rect,
            CornerRadius::ZERO,
            if track.kind == TrackKind::Master {
                theme::LANE_ALT
            } else {
                theme::LANE
            },
        );
        painter.rect_filled(
            Rect::from_min_size(
                egui::pos2(header_rect.left(), row_rect.top()),
                Vec2::new(3.0, row_rect.height()),
            ),
            CornerRadius::ZERO,
            track.color.map_or_else(
                || track_color(track.kind),
                |color| Color32::from_rgba_unmultiplied(color.r, color.g, color.b, 255),
            ),
        );
        let depth = track_depth(doc, track_id) as f32;
        let name_x = header_rect.left() + 11.0 + depth * 10.0;
        let name_rect = Rect::from_min_max(
            egui::pos2(name_x, header_rect.top() + 4.0),
            egui::pos2(header_rect.right() - 54.0, header_rect.top() + 22.0),
        );
        painter.with_clip_rect(name_rect).text(
            egui::pos2(name_x, header_rect.top() + 6.0),
            Align2::LEFT_TOP,
            &track.name,
            FontId::proportional(12.5),
            if selected_track {
                Color32::WHITE
            } else {
                theme::TEXT_MUTED
            },
        );
        painter.text(
            egui::pos2(name_x, header_rect.top() + 23.0),
            Align2::LEFT_TOP,
            track.kind.name().to_uppercase(),
            FontId::monospace(8.5),
            theme::TEXT_MUTED,
        );
        let mute_rect = Rect::from_min_size(
            egui::pos2(header_rect.right() - 45.0, header_rect.top() + 6.0),
            Vec2::new(18.0, 16.0),
        );
        let solo_rect = mute_rect.translate(Vec2::new(22.0, 0.0));
        painter.rect_filled(
            mute_rect,
            CornerRadius::same(3),
            if track.mixer.mute {
                theme::PLAYHEAD
            } else {
                theme::GRID
            },
        );
        painter.text(
            mute_rect.center(),
            Align2::CENTER_CENTER,
            "M",
            FontId::monospace(9.0),
            if track.mixer.mute {
                Color32::BLACK
            } else {
                theme::TEXT_MUTED
            },
        );
        painter.rect_filled(
            solo_rect,
            CornerRadius::same(3),
            if track.mixer.solo {
                theme::ACCENT
            } else {
                theme::GRID
            },
        );
        painter.text(
            solo_rect.center(),
            Align2::CENTER_CENTER,
            "S",
            FontId::monospace(9.0),
            if track.mixer.solo {
                Color32::BLACK
            } else {
                theme::TEXT_MUTED
            },
        );

        let volume_rect = Rect::from_min_size(
            egui::pos2(name_x, header_rect.bottom() - 17.0),
            Vec2::new(95.0, 14.0),
        );
        let mut volume = track.mixer.volume_db.0;
        let volume_changed = ui
            .put(
                volume_rect,
                egui::Slider::new(&mut volume, -60.0..=6.0)
                    .show_value(false)
                    .trailing_fill(true),
            )
            .on_hover_text(format_db(notwig_core::Db(volume)))
            .changed();
        if volume_changed {
            self.emit(UiIntent::SetTrackMixer {
                track_id,
                volume_db: Some(notwig_core::Db(volume)),
                pan: None,
                mute: None,
                solo: None,
            });
        }
        let meter_rect = Rect::from_min_size(
            egui::pos2(header_rect.right() - 24.0, header_rect.bottom() - 17.0),
            Vec2::new(14.0, 14.0),
        );
        let meter = runtime.meters.get(&track_id).copied().unwrap_or_default();
        painter.rect_filled(meter_rect, CornerRadius::same(2), theme::GRID);
        let meter_fill = Rect::from_min_max(
            meter_rect.min,
            egui::pos2(
                meter_rect.left()
                    + meter_rect.width() * meter.peak_l.max(meter.peak_r).clamp(0.0, 1.0),
                meter_rect.bottom(),
            ),
        );
        painter.rect_filled(meter_fill, CornerRadius::same(2), theme::CLIP_MIDI);

        let beats_per_bar = doc.meta.time_sig_num.max(1);
        let visible_beats = f64::from((lane_rect.width() / ppb).max(1.0));
        let step = grid.as_f64().max(1.0 / 256.0);
        let bar_len = f64::from(beats_per_bar);
        let first_index = (start.as_f64() / step).floor() as i64;
        let last_index = ((start.as_f64() + visible_beats) / step).ceil() as i64 + 1;
        for index in first_index..=last_index {
            let beat = index as f64 * step;
            if beat < 0.0 {
                continue;
            }
            let x = lane_rect.left() + (beat - start.as_f64()) as f32 * ppb;
            if x < lane_rect.left() - 1.0 || x > lane_rect.right() + 1.0 {
                continue;
            }
            let strong = (beat / bar_len).fract().abs() < 1e-6;
            let beat_line = (beat.fract()).abs() < 1e-6;
            painter.line_segment(
                [
                    egui::pos2(x, lane_rect.top()),
                    egui::pos2(x, lane_rect.bottom()),
                ],
                Stroke::new(
                    1.0_f32,
                    if strong {
                        theme::GRID_STRONG
                    } else if beat_line {
                        theme::GRID
                    } else {
                        theme::GRID.gamma_multiply(0.55)
                    },
                ),
            );
        }

        let header_resp = ui.interact(
            header_rect,
            Id::new(("track-header", track_id)),
            Sense::click_and_drag(),
        );
        if header_resp.clicked() {
            let additive = ui.input(|i| i.modifiers.command || i.modifiers.shift);
            self.state.selection.select_track(track_id, additive);
            self.emit(UiIntent::SelectTrack(track_id));
        }
        if header_resp.drag_started() && track.kind != TrackKind::Master {
            self.track_drag = Some(track_id);
            self.state.selection.select_track(track_id, false);
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        if header_resp.dragged() {
            if let Some(pointer) = ui.input(|i| i.pointer.hover_pos()) {
                let painter = ui.painter();
                painter.line_segment(
                    [
                        egui::pos2(row_rect.left(), pointer.y),
                        egui::pos2(row_rect.right(), pointer.y),
                    ],
                    Stroke::new(2.0_f32, theme::ACCENT),
                );
            }
        }
        if header_resp.drag_stopped()
            && self.track_drag.take().is_some()
            && let Some(pointer) = ui.input(|i| i.pointer.hover_pos())
        {
            let order = doc.track_order.depth_first();
            let mut index = 0usize;
            for id in &order {
                if let Some(row) = self.track_rows.get(id) {
                    if pointer.y > row.center().y {
                        index += 1;
                    }
                }
            }
            self.emit(UiIntent::MoveTrack {
                id: track_id,
                index,
            });
        }
        if ui
            .interact(mute_rect, Id::new(("mute", track_id)), Sense::click())
            .clicked()
        {
            self.emit(UiIntent::SetTrackMixer {
                track_id,
                volume_db: None,
                pan: None,
                mute: Some(!track.mixer.mute),
                solo: None,
            });
        }
        if ui
            .interact(solo_rect, Id::new(("solo", track_id)), Sense::click())
            .clicked()
        {
            self.emit(UiIntent::SetTrackMixer {
                track_id,
                volume_db: None,
                pan: None,
                mute: None,
                solo: Some(!track.mixer.solo),
            });
        }

        let lane_resp = ui.interact(
            lane_rect,
            Id::new(("lane", track_id)),
            Sense::click_and_drag(),
        );
        if lane_resp.double_clicked() {
            if let Some(pos) = lane_resp.interact_pointer_pos() {
                self.lane_double_click(
                    doc,
                    track_id,
                    pos,
                    lane_rect,
                    start,
                    ppb,
                    grid,
                    ui.input(|i| i.modifiers.shift),
                );
            }
        }
        if lane_resp.clicked() {
            if let Some(pos) = lane_resp.interact_pointer_pos() {
                let shift = ui.input(|i| i.modifiers.shift);
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
                if pos.y > lane_rect.center().y {
                    let shift = ui.input(|i| i.modifiers.shift);
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
        }
        if lane_resp.dragged() {
            if let Some(drag) = self.time_drag {
                if let Some(pos) = lane_resp.interact_pointer_pos() {
                    let shift = ui.input(|i| i.modifiers.shift);
                    let beat = snap_time(
                        self.state.viewport.x_to_beat(pos.x, lane_rect.left()),
                        grid,
                        shift,
                    );
                    let (start_mus, end_mus) = if drag.start_beat <= beat {
                        (drag.start_beat, beat)
                    } else {
                        (beat, drag.start_beat)
                    };
                    if let Ok(range) = RangeMus::new(start_mus, end_mus) {
                        let min_y = drag.start_y.min(pos.y);
                        let max_y = drag.start_y.max(pos.y);
                        let mut tracks = Vec::new();
                        for id in doc.track_order.depth_first() {
                            if let Some(row) = self.track_rows.get(&id) {
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
        lane_resp.context_menu(|ui| {
            if track.kind != TrackKind::Master {
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

        let clip_ids = doc
            .arranger
            .lanes
            .get(&track_id)
            .map(|lane| lane.clips.clone())
            .unwrap_or_default();
        for clip_id in clip_ids {
            self.draw_clip(
                ui, doc, track_id, clip_id, lane_rect, start, ppb, runtime, grid,
            );
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

        if let Some(time) = &self.state.selection.time {
            if time.contains_track(track_id) {
                let x1 =
                    lane_rect.left() + (time.range.start.as_f64() - start.as_f64()) as f32 * ppb;
                let x2 = lane_rect.left() + (time.range.end.as_f64() - start.as_f64()) as f32 * ppb;
                let rect = Rect::from_min_max(
                    egui::pos2(x1.max(lane_rect.left()), lane_rect.top()),
                    egui::pos2(x2.min(lane_rect.right()), lane_rect.bottom()),
                );
                painter.rect_filled(
                    rect,
                    CornerRadius::ZERO,
                    Color32::from_rgba_unmultiplied(255, 210, 120, 26),
                );
            }
        }
    }

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
        let beat = snap_time(
            TimeMus::new((beat * 1_000_000.0).round() as i64, 1_000_000),
            grid,
            bypass_snap || alt,
        );
        match track.kind {
            TrackKind::Instrument => {
                if let Ok(range) = RangeMus::new(beat, beat + TimeMus::new(4, 1)) {
                    self.emit(UiIntent::CreateMidiClip { track_id, range });
                }
            }
            TrackKind::Audio => {
                let asset = doc
                    .assets
                    .assets
                    .values()
                    .find(|asset| asset.kind == notwig_core::AssetKind::Audio)
                    .map(|asset| asset.id);
                if let Some(asset_id) = asset {
                    self.emit(UiIntent::InsertAudioClip { track_id, asset_id });
                } else {
                    self.state.notify(
                        crate::state::NotificationKind::Info,
                        "Import audio from File → Import Audio, then double-click the lane.",
                    );
                }
            }
            _ => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_clip(
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
        let preview = self
            .clip_preview
            .filter(|(id, _, _)| *id == clip_id)
            .map(|(_, range, _)| range);
        let range = preview.unwrap_or(clip.range);
        let x1 = lane_rect.left() + (range.start.as_f64() - start.as_f64()) as f32 * ppb;
        let x2 = lane_rect.left() + (range.end.as_f64() - start.as_f64()) as f32 * ppb;
        let clip_rect = Rect::from_min_max(
            egui::pos2(x1, lane_rect.top() + 3.0),
            egui::pos2(x2.max(x1 + 1.0), lane_rect.bottom() - 3.0),
        );
        let visible = clip_rect.intersect(lane_rect);
        if visible.width() < 2.0 {
            return;
        }
        let selected = self.state.selection.clips.contains(&clip_id);
        let color = clip_color(clip.kind);
        let color = if clip.muted {
            color.gamma_multiply(0.35)
        } else {
            color
        };
        let painter = ui.painter().with_clip_rect(lane_rect);
        painter.rect_filled(clip_rect, CornerRadius::same(3), color);
        let name_height = 14.0_f32.min(clip_rect.height() * 0.42);
        let name_rect = Rect::from_min_max(
            clip_rect.min,
            egui::pos2(clip_rect.right(), clip_rect.top() + name_height),
        );
        let body_rect = Rect::from_min_max(
            egui::pos2(clip_rect.left(), name_rect.bottom()),
            clip_rect.max,
        );
        painter.rect_filled(
            name_rect,
            CornerRadius {
                nw: 3,
                ne: 3,
                sw: 0,
                se: 0,
            },
            Color32::from_black_alpha(52),
        );
        match &clip.payload {
            ClipPayload::Midi(midi) => {
                draw_midi_preview(&painter, body_rect, clip.range, midi);
            }
            ClipPayload::Audio(audio) => {
                if let Some(peaks) = runtime.waveforms.get(&audio.asset_id) {
                    draw_waveform(
                        &painter,
                        body_rect,
                        peaks,
                        audio.source_start_sample,
                        audio.source_end_sample,
                    );
                }
            }
        }
        painter.rect_stroke(
            clip_rect,
            CornerRadius::same(3),
            Stroke::new(
                if selected { 1.8_f32 } else { 1.0_f32 },
                if selected {
                    theme::SELECTION
                } else {
                    Color32::from_black_alpha(120)
                },
            ),
            StrokeKind::Inside,
        );
        let label = clip.name.clone().unwrap_or_else(|| match &clip.payload {
            ClipPayload::Midi(midi) => format!("MIDI · {} notes", midi.notes.len()),
            ClipPayload::Audio(audio) => format!(
                "AUDIO · {:.1}s",
                audio.source_duration() as f64 / f64::from(doc.meta.sample_rate.value())
            ),
        });
        painter.with_clip_rect(name_rect).text(
            egui::pos2(name_rect.left() + 8.0, name_rect.center().y),
            Align2::LEFT_CENTER,
            label,
            FontId::monospace(9.5),
            Color32::from_black_alpha(235),
        );
        if preview.is_some() {
            painter.rect_stroke(
                clip_rect,
                CornerRadius::same(3),
                Stroke::new(1.5_f32, theme::ACCENT),
                StrokeKind::Inside,
            );
        }

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
            if let Some(pos) = ui
                .input(|i| i.pointer.press_origin())
                .or_else(|| name_response.interact_pointer_pos())
            {
                self.begin_clip_drag(clip, clip_id, track_id, ClipDragMode::Move, pos);
            }
        }
        self.update_clip_drag(ui, doc, &name_response, clip, clip_id, lane_rect, ppb, grid);

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
                if let Some(pos) = ui
                    .input(|i| i.pointer.press_origin())
                    .or_else(|| response.interact_pointer_pos())
                {
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
        if response.dragged() {
            if let Some(drag) = self.drag.filter(|drag| drag.clip_id == clip_id) {
                let current = response
                    .interact_pointer_pos()
                    .or_else(|| ui.input(|i| i.pointer.hover_pos()))
                    .unwrap_or(drag.start_pointer);
                let alt = ui.input(|i| i.modifiers.alt);
                let delta = f64::from(current.x - drag.start_pointer.x) / f64::from(ppb);
                let range = drag_range(drag, delta, alt, grid);
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

    fn draw_drop_preview(&self, ui: &mut egui::Ui, doc: &ProjectDoc, header_w: f32) {
        let Some(preview) = &self.drag_preview else {
            return;
        };
        let painter = ui.painter().clone();
        let order: Vec<TrackId> = doc.track_order.depth_first();
        let lane = match preview.target {
            crate::intent::DropAudioTarget::Track(track_id) => {
                self.track_rows.get(&track_id).map(|row| {
                    Rect::from_min_max(
                        egui::pos2(row.left() + header_w, row.top() + 3.0),
                        egui::pos2(row.right() - 1.0, row.bottom() - 3.0),
                    )
                })
            }
            crate::intent::DropAudioTarget::NewTrack { index } => {
                let y = order
                    .get(index)
                    .and_then(|id| self.track_rows.get(id))
                    .map(egui::Rect::top)
                    .or_else(|| {
                        self.track_rows
                            .values()
                            .map(egui::Rect::bottom)
                            .reduce(f32::max)
                    })
                    .unwrap_or(ui.max_rect().top());
                painter.line_segment(
                    [
                        egui::pos2(ui.max_rect().left(), y),
                        egui::pos2(ui.max_rect().right(), y),
                    ],
                    Stroke::new(2.0_f32, theme::ACCENT),
                );
                Some(Rect::from_min_max(
                    egui::pos2(ui.max_rect().left() + header_w, y + 4.0),
                    egui::pos2(ui.max_rect().right() - 1.0, y + 4.0 + ROW_H - 8.0),
                ))
            }
        };
        let Some(lane) = lane else {
            return;
        };
        let x1 = self
            .state
            .viewport
            .beat_to_x(preview.at, self.timeline_left);
        let end = preview.at + preview.duration;
        let x2 = self.state.viewport.beat_to_x(end, self.timeline_left);
        let rect = Rect::from_min_max(
            egui::pos2(x1.max(lane.left()), lane.top()),
            egui::pos2(x2.max(x1 + 8.0).min(lane.right()), lane.bottom()),
        );
        painter.rect_filled(
            rect,
            CornerRadius::same(3),
            theme::ACCENT.gamma_multiply(0.28),
        );
        painter.rect_stroke(
            rect,
            CornerRadius::same(3),
            Stroke::new(1.5_f32, theme::ACCENT),
            StrokeKind::Inside,
        );
        if let Some(name) = preview.path.file_name().and_then(|name| name.to_str()) {
            painter.with_clip_rect(rect).text(
                egui::pos2(rect.left() + 5.0, rect.center().y),
                Align2::LEFT_CENTER,
                name,
                FontId::monospace(10.0),
                Color32::WHITE,
            );
        }
    }

    fn timeline_scrollbar(
        &mut self,
        ui: &mut egui::Ui,
        bar_rect: Rect,
        doc: &ProjectDoc,
        header_w: f32,
        ppb: f32,
    ) {
        let painter = ui.painter();
        painter.rect_filled(bar_rect, CornerRadius::ZERO, theme::PANEL_LIGHT);
        let track = Rect::from_min_max(
            egui::pos2(bar_rect.left() + header_w + 4.0, bar_rect.top() + 3.0),
            egui::pos2(bar_rect.right() - 4.0, bar_rect.bottom() - 3.0),
        );
        let content_end = doc
            .arranger
            .clips
            .values()
            .map(|clip| clip.range.end.as_f64())
            .fold(8.0_f64, f64::max);
        let visible = f64::from(((bar_rect.right() - bar_rect.left() - header_w) / ppb).max(1.0));
        let content = (content_end + 4.0).max(visible);
        let start = self
            .state
            .viewport
            .start
            .as_f64()
            .clamp(0.0, (content - visible).max(0.0));
        let thumb_x = track.left() + (start / content) as f32 * track.width();
        let thumb_w = ((visible / content) as f32 * track.width())
            .max(24.0)
            .min(track.width());
        let thumb = Rect::from_min_size(
            egui::pos2(thumb_x.min(track.right() - thumb_w), track.top()),
            Vec2::new(thumb_w, track.height()),
        );
        painter.rect_filled(track, CornerRadius::same(3), theme::GRID);
        painter.rect_filled(thumb, CornerRadius::same(3), theme::TEXT_MUTED);
        let response = ui.interact(
            bar_rect,
            Id::new("arranger_hscroll"),
            Sense::click_and_drag(),
        );
        if response.clicked() || response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                let fraction = f64::from(((pos.x - track.left()) / track.width()).clamp(0.0, 1.0));
                let max_start = (content - visible).max(0.0);
                self.state.viewport.start = TimeMus::new(
                    ((fraction * max_start) * 1_000_000.0).round() as i64,
                    1_000_000,
                );
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

fn track_color(kind: TrackKind) -> Color32 {
    match kind {
        TrackKind::Audio => theme::CLIP_AUDIO,
        TrackKind::Instrument => theme::CLIP_MIDI,
        TrackKind::Group => Color32::from_rgb(116, 120, 136),
        TrackKind::BusFx => Color32::from_rgb(154, 122, 190),
        TrackKind::Master => theme::ACCENT,
    }
}

fn draw_midi_preview(
    painter: &egui::Painter,
    rect: Rect,
    range: RangeMus,
    midi: &notwig_core::MidiClipData,
) {
    if rect.height() < 12.0 {
        return;
    }
    let clip_start = range.start.as_f64();
    let clip_end = range.end.as_f64();
    let clip_len = range.length().as_f64().max(1e-6);
    for note in &midi.notes {
        let relative = (note.start - midi.content_start).as_f64();
        let abs_start = clip_start + relative;
        let abs_end = abs_start + note.length.as_f64();
        let start = abs_start.max(clip_start);
        let end = abs_end.min(clip_end);
        if end <= start {
            continue;
        }
        let x1 = rect.left() + ((start - clip_start) / clip_len) as f32 * rect.width();
        let x2 = rect.left() + ((end - clip_start) / clip_len) as f32 * rect.width();
        let pitch = f32::from(note.pitch);
        let y = rect.bottom() - 5.0 - (pitch / 127.0) * (rect.height() - 18.0);
        painter.rect_filled(
            Rect::from_min_max(
                egui::pos2(x1, y - 1.5),
                egui::pos2(x2.max(x1 + 1.5), y + 1.5),
            ),
            CornerRadius::same(1),
            Color32::from_black_alpha(150),
        );
    }
}

fn draw_waveform(
    painter: &egui::Painter,
    rect: Rect,
    peaks: &[f32],
    source_start: u64,
    source_end: u64,
) {
    if peaks.is_empty() || source_end <= source_start || rect.width() < 2.0 {
        return;
    }
    let duration = (source_end - source_start) as f32;
    let center = rect.center().y;
    let half = rect.height() * 0.34;
    let steps = (rect.width() / 2.0).max(1.0) as usize;
    for step in 0..steps {
        let t = step as f32 / steps as f32;
        let source = source_start as f32 + t * duration;
        let index = (((source - source_start as f32) / duration) * peaks.len() as f32) as usize;
        let peak = peaks
            .get(index.min(peaks.len() - 1))
            .copied()
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        let x = rect.left() + t * rect.width();
        painter.line_segment(
            [
                egui::pos2(x, center - peak * half),
                egui::pos2(x, center + peak * half),
            ],
            Stroke::new(1.0_f32, Color32::from_black_alpha(120)),
        );
    }
}

fn drag_range(drag: ClipDrag, delta: f64, alt: bool, grid: TimeMus) -> RangeMus {
    let origin_start = drag.origin.start.as_f64();
    let origin_end = drag.origin.end.as_f64();
    let length = (origin_end - origin_start).max(1.0 / 16.0);
    match drag.mode {
        ClipDragMode::Move => {
            let start = snap_time(
                TimeMus::new(
                    (((origin_start + delta) * 1_000_000.0).round()) as i64,
                    1_000_000,
                ),
                grid,
                alt,
            );
            RangeMus::new(
                start,
                start + TimeMus::new((length * 1_000_000.0) as i64, 1_000_000),
            )
            .unwrap_or(drag.origin)
        }
        ClipDragMode::TrimStart => {
            let start = snap_time(
                TimeMus::new(
                    (((origin_start + delta) * 1_000_000.0).round()) as i64,
                    1_000_000,
                ),
                grid,
                alt,
            );
            let end = TimeMus::new((origin_end * 1_000_000.0) as i64, 1_000_000);
            if start < end {
                RangeMus::new(start, end).unwrap_or(drag.origin)
            } else {
                drag.origin
            }
        }
        ClipDragMode::TrimEnd => {
            let end = snap_time(
                TimeMus::new(
                    (((origin_end + delta) * 1_000_000.0).round()) as i64,
                    1_000_000,
                ),
                grid,
                alt,
            );
            let start = TimeMus::new((origin_start * 1_000_000.0) as i64, 1_000_000);
            if end > start {
                RangeMus::new(start, end).unwrap_or(drag.origin)
            } else {
                drag.origin
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_keeps_pointer_beat_stable() {
        let mut ui = NotwigUi::default();
        ui.timeline_left = 230.0;
        ui.state.viewport.pixels_per_beat = 64.0;
        ui.state.viewport.start = TimeMus::zero();
        ui.state.viewport.zoom_at(430.0, ui.timeline_left, 2.0);
        assert!((ui.state.viewport.pixels_per_beat - 128.0).abs() < 1e-3);
        let beat = ui.state.viewport.start.as_f64()
            + f64::from((430.0 - ui.timeline_left) / ui.state.viewport.pixels_per_beat);
        assert!((beat - 3.125).abs() < 1e-6);
    }

    #[test]
    fn panning_clamps_at_zero() {
        let mut ui = NotwigUi::default();
        ui.state.viewport.start = TimeMus::new(10, 1);
        ui.state.viewport.pan_beats(-4.0);
        assert_eq!(ui.state.viewport.start, TimeMus::new(6, 1));
        ui.state.viewport.pan_beats(-100.0);
        assert_eq!(ui.state.viewport.start, TimeMus::zero());
    }
}
