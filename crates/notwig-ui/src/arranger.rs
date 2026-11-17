use egui::{Align2, Color32, CornerRadius, FontId, Id, Rect, Sense, Stroke, StrokeKind, Vec2};
use notwig_core::{ClipId, ProjectDoc, RangeMus, TimeMus, TrackId, TrackKind};

use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::shell::{NotwigUi, clip_color, track_depth};
use crate::theme;
use crate::widgets::snap_16;

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
}

impl NotwigUi {
    pub(crate) fn arranger_view(
        &mut self,
        ui: &mut egui::Ui,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
    ) {
        let header_w = 190.0;
        let ppb = self.state.viewport.pixels_per_beat.clamp(4.0, 400.0);
        let start = self.state.viewport.start;
        let beats_per_bar = doc.meta.time_sig_num.max(1);
        let width = ui.available_width().max(320.0);
        let (ruler_rect, ruler_resp) =
            ui.allocate_exact_size(Vec2::new(width, 28.0), Sense::click());
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
        let timeline_left = ruler_rect.left() + header_w;
        let visible_beats = f64::from(((ruler_rect.right() - timeline_left) / ppb).max(1.0));
        let first_beat = start.as_f64().floor().max(0.0) as i64;
        let last_beat = (start.as_f64() + visible_beats).ceil() as i64 + 1;
        for beat in first_beat..=last_beat {
            let x = timeline_left + (beat as f64 - start.as_f64()) as f32 * ppb;
            if x < timeline_left - 1.0 || x > ruler_rect.right() + 1.0 {
                continue;
            }
            let strong = beat % i64::from(beats_per_bar) == 0;
            painter.line_segment(
                [
                    egui::pos2(x, ruler_rect.top() + if strong { 10.0 } else { 18.0 }),
                    egui::pos2(x, ruler_rect.bottom()),
                ],
                Stroke::new(
                    1.0_f32,
                    if strong {
                        theme::GRID_STRONG
                    } else {
                        theme::GRID
                    },
                ),
            );
            if strong {
                let bar = beat / i64::from(beats_per_bar) + 1;
                painter.text(
                    egui::pos2(x + 4.0, ruler_rect.top() + 4.0),
                    Align2::LEFT_TOP,
                    bar.to_string(),
                    FontId::monospace(10.0),
                    theme::TEXT_MUTED,
                );
            }
        }
        let playhead_x = timeline_left + (runtime.playhead.as_f64() - start.as_f64()) as f32 * ppb;
        if playhead_x >= timeline_left && playhead_x <= ruler_rect.right() {
            painter.line_segment(
                [
                    egui::pos2(playhead_x, ruler_rect.top()),
                    egui::pos2(playhead_x, ruler_rect.bottom()),
                ],
                Stroke::new(1.5_f32, theme::PLAYHEAD),
            );
        }
        if ruler_resp.clicked() {
            if let Some(pos) = ruler_resp.interact_pointer_pos() {
                if pos.x > timeline_left {
                    let beat = start.as_f64() + f64::from((pos.x - timeline_left) / ppb);
                    let alt = ui.input(|i| i.modifiers.alt);
                    self.emit(UiIntent::SetPlayhead(snap_16(
                        TimeMus::new((beat * 1_000_000.0).round() as i64, 1_000_000),
                        alt,
                    )));
                }
            }
        }
        let scroll = ui.input(|i| i.raw_scroll_delta.y);
        if ruler_resp.hovered() && scroll.abs() > 0.0 {
            let factor = (scroll * 0.0015).exp();
            self.state.viewport.pixels_per_beat = (ppb * factor).clamp(4.0, 400.0);
        }

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for track_id in doc.track_order.depth_first() {
                    self.track_row(ui, doc, track_id, header_w, ppb, start, runtime);
                }
            });
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
    ) {
        let Some(track) = doc.tracks.get(&track_id) else {
            return;
        };
        let width = ui.available_width().max(320.0);
        let (row_rect, _) = ui.allocate_exact_size(Vec2::new(width, 38.0), Sense::hover());
        let header_rect = Rect::from_min_size(
            row_rect.min,
            Vec2::new(header_w.min(row_rect.width()), row_rect.height()),
        );
        let lane_rect = Rect::from_min_max(
            egui::pos2(header_rect.right(), row_rect.top()),
            row_rect.max,
        );
        let selected_track = self.state.selection.tracks.contains(&track_id)
            || self.state.selection.primary_track == Some(track_id);
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
        painter.rect_filled(lane_rect, CornerRadius::ZERO, theme::LANE);
        let depth = track_depth(doc, track_id) as f32;
        let name_x = header_rect.left() + 10.0 + depth * 10.0;
        painter.text(
            egui::pos2(name_x, header_rect.top() + 7.0),
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
            egui::pos2(name_x, header_rect.bottom() - 6.0),
            Align2::LEFT_BOTTOM,
            track.kind.name().to_uppercase(),
            FontId::monospace(9.0),
            theme::TEXT_MUTED,
        );
        if let Some(color) = track.color {
            painter.rect_filled(
                Rect::from_min_size(
                    egui::pos2(header_rect.left(), row_rect.top()),
                    Vec2::new(3.0, row_rect.height()),
                ),
                CornerRadius::ZERO,
                Color32::from_rgba_unmultiplied(color.r, color.g, color.b, 255),
            );
        }

        let mute_rect = Rect::from_min_size(
            egui::pos2(header_rect.right() - 46.0, header_rect.top() + 11.0),
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

        let beats_per_bar = doc.meta.time_sig_num.max(1);
        let visible_beats = f64::from((lane_rect.width() / ppb).max(1.0));
        let first_beat = start.as_f64().floor().max(0.0) as i64;
        let last_beat = (start.as_f64() + visible_beats).ceil() as i64 + 1;
        for beat in first_beat..=last_beat {
            let x = lane_rect.left() + (beat as f64 - start.as_f64()) as f32 * ppb;
            if x < lane_rect.left() - 1.0 || x > lane_rect.right() + 1.0 {
                continue;
            }
            let strong = beat % i64::from(beats_per_bar) == 0;
            painter.line_segment(
                [
                    egui::pos2(x, lane_rect.top()),
                    egui::pos2(x, lane_rect.bottom()),
                ],
                Stroke::new(
                    1.0_f32,
                    if strong {
                        theme::GRID_STRONG
                    } else {
                        theme::GRID
                    },
                ),
            );
        }

        let header_resp = ui.interact(
            header_rect,
            Id::new(("track-header", track_id)),
            Sense::click(),
        );
        if header_resp.clicked() {
            self.state.selection.select_only_track(track_id);
            self.state.selection.clips.clear();
            self.emit(UiIntent::SelectTrack(track_id));
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
                if track.kind == TrackKind::Instrument {
                    let alt = ui.input(|i| i.modifiers.alt);
                    let beat = start.as_f64() + f64::from((pos.x - lane_rect.left()) / ppb);
                    let beat = snap_16(
                        TimeMus::new((beat * 1_000_000.0).round() as i64, 1_000_000),
                        alt,
                    );
                    let end = beat + TimeMus::new(4, 1);
                    if let Ok(range) = RangeMus::new(beat, end) {
                        self.emit(UiIntent::CreateMidiClip { track_id, range });
                    }
                } else {
                    self.state.notify(
                        crate::state::NotificationKind::Info,
                        "Audio clips require an imported asset. Use the browser after importing audio.",
                    );
                }
            }
        }

        let clip_ids = doc
            .arranger
            .lanes
            .get(&track_id)
            .map(|lane| lane.clips.clone())
            .unwrap_or_default();
        for clip_id in clip_ids {
            let Some(clip) = doc.arranger.clips.get(&clip_id) else {
                continue;
            };
            let preview = self
                .clip_preview
                .filter(|(id, _, _)| *id == clip_id)
                .map(|(_, range, _)| range);
            let range = preview.unwrap_or(clip.range);
            let x1 = lane_rect.left() + (range.start.as_f64() - start.as_f64()) as f32 * ppb;
            let x2 = lane_rect.left() + (range.end.as_f64() - start.as_f64()) as f32 * ppb;
            let clip_rect = Rect::from_min_max(
                egui::pos2(x1.max(lane_rect.left() + 1.0), lane_rect.top() + 3.0),
                egui::pos2(x2.min(lane_rect.right() - 1.0), lane_rect.bottom() - 3.0),
            );
            if clip_rect.width() < 2.0 {
                continue;
            }
            let selected = self.state.selection.clips.contains(&clip_id);
            let color = clip_color(clip.kind);
            let color = if clip.muted {
                color.gamma_multiply(0.35)
            } else {
                color
            };
            painter.rect_filled(clip_rect, CornerRadius::same(3), color);
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
                notwig_core::ClipPayload::Midi(midi) => {
                    format!("MIDI · {} notes", midi.notes.len())
                }
                notwig_core::ClipPayload::Audio(_) => "AUDIO".into(),
            });
            painter.text(
                egui::pos2(clip_rect.left() + 5.0, clip_rect.center().y),
                Align2::LEFT_CENTER,
                label,
                FontId::proportional(10.5),
                Color32::from_black_alpha(220),
            );
            if preview.is_some() {
                painter.rect_stroke(
                    clip_rect,
                    CornerRadius::same(3),
                    Stroke::new(1.5_f32, theme::ACCENT),
                    StrokeKind::Inside,
                );
            }

            let response = ui.interact(
                clip_rect,
                Id::new(("clip", clip_id)),
                Sense::click_and_drag(),
            );
            if response.clicked() {
                self.state.selection.clips.clear();
                self.state.selection.clips.insert(clip_id);
                self.state.selection.select_only_track(track_id);
                self.state.selection.devices.clear();
                self.emit(UiIntent::SelectClip(clip_id));
            }
            if response.drag_started() {
                if let Some(pos) = response.interact_pointer_pos() {
                    let edge = 7.0;
                    let mode = if (pos.x - clip_rect.left()).abs() < edge {
                        ClipDragMode::TrimStart
                    } else if (pos.x - clip_rect.right()).abs() < edge {
                        ClipDragMode::TrimEnd
                    } else {
                        ClipDragMode::Move
                    };
                    self.drag = Some(ClipDrag {
                        clip_id,
                        track_id,
                        mode,
                        origin: clip.range,
                    });
                    self.state.selection.clips.clear();
                    self.state.selection.clips.insert(clip_id);
                    self.state.selection.select_only_track(track_id);
                }
            }
            if response.dragged() {
                if let Some(drag) = self.drag {
                    let alt = ui.input(|i| i.modifiers.alt);
                    let delta = f64::from(response.drag_delta().x) / f64::from(ppb);
                    let range = drag_range(drag, delta, alt);
                    self.clip_preview = Some((clip_id, range, track_id));
                }
            }
            if response.drag_stopped() {
                if let Some((id, range, target_track)) = self.clip_preview.take() {
                    if let Some(drag) = self.drag.take() {
                        if range != drag.origin {
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

        let playhead_x =
            lane_rect.left() + (runtime.playhead.as_f64() - start.as_f64()) as f32 * ppb;
        if playhead_x >= lane_rect.left() && playhead_x <= lane_rect.right() {
            painter.line_segment(
                [
                    egui::pos2(playhead_x, lane_rect.top()),
                    egui::pos2(playhead_x, lane_rect.bottom()),
                ],
                Stroke::new(1.2_f32, theme::PLAYHEAD),
            );
        }
    }
}

fn drag_range(drag: ClipDrag, delta: f64, alt: bool) -> RangeMus {
    let origin_start = drag.origin.start.as_f64();
    let origin_end = drag.origin.end.as_f64();
    let length = (origin_end - origin_start).max(1.0 / 16.0);
    match drag.mode {
        ClipDragMode::Move => {
            let start = snap_16(
                TimeMus::new(
                    (((origin_start + delta) * 1_000_000.0).round()) as i64,
                    1_000_000,
                ),
                alt,
            );
            RangeMus::new(
                start,
                start + TimeMus::new((length * 1_000_000.0) as i64, 1_000_000),
            )
            .unwrap_or(drag.origin)
        }
        ClipDragMode::TrimStart => {
            let start = snap_16(
                TimeMus::new(
                    (((origin_start + delta) * 1_000_000.0).round()) as i64,
                    1_000_000,
                ),
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
            let end = snap_16(
                TimeMus::new(
                    (((origin_end + delta) * 1_000_000.0).round()) as i64,
                    1_000_000,
                ),
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
