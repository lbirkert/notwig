use egui::{Align2, Color32, CornerRadius, FontId, Id, Rect, Sense, Stroke, StrokeKind, Vec2};
use notwig_core::{ClipId, ProjectDoc, RangeMus, TimeMus, TrackId, TrackKind};

use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::shell::{NotwigUi, track_depth};
use crate::theme;
use crate::timeline::TimelineGrid;
use crate::widgets::snap_time;

use super::{ROW_H, TRACK_INDENT, TimeDrag, TrackDrop};

impl NotwigUi {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn track_row(
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
        let depth = track_depth(doc, track_id) as f32;
        let header_left = row_rect.left() + (depth * 14.0).min(header_w * 0.35);
        let header_rect = Rect::from_min_max(
            egui::pos2(header_left, row_rect.top()),
            egui::pos2(
                row_rect.left() + header_w.min(row_rect.width()),
                row_rect.bottom(),
            ),
        );
        let lane_rect = Rect::from_min_max(
            egui::pos2(
                row_rect.left() + header_w.min(row_rect.width()),
                row_rect.top(),
            ),
            row_rect.max,
        );
        let selected_track = self.state.selection.tracks.contains(&track_id)
            || self.state.selection.tracks.primary() == Some(track_id);
        let painter = ui.painter().clone();
        painter.rect_filled(
            header_rect,
            CornerRadius::ZERO,
            if selected_track {
                Color32::from_rgb(38, 40, 46)
            } else {
                Color32::BLACK
            },
        );
        let track_color = track.color.map_or_else(
            || track_color(track.kind),
            |color| Color32::from_rgba_unmultiplied(color.r, color.g, color.b, 255),
        );
        painter.rect_filled(
            Rect::from_min_size(
                egui::pos2(header_rect.left(), row_rect.top()),
                Vec2::new(4.0, row_rect.height()),
            ),
            CornerRadius::ZERO,
            track_color,
        );
        draw_track_prefixes(&painter, doc, track, row_rect, depth);
        let meter_w = 12.0;
        let content_left = header_rect.left() + 10.0;
        let content_right = header_rect.right() - meter_w - 8.0;
        let name_color = if selected_track {
            Color32::WHITE
        } else {
            theme::TEXT_MUTED
        };
        let name_rect = Rect::from_min_max(
            egui::pos2(content_left, header_rect.top() + 4.0),
            egui::pos2(content_right - 68.0, header_rect.top() + 26.0),
        );
        painter.with_clip_rect(name_rect).text(
            egui::pos2(content_left, header_rect.top() + 6.0),
            Align2::LEFT_TOP,
            &track.name,
            FontId::proportional(13.0),
            name_color,
        );

        let mute_rect = Rect::from_center_size(
            egui::pos2(content_right - 9.0, header_rect.top() + 16.0),
            Vec2::splat(18.0),
        );
        let solo_rect = mute_rect.translate(Vec2::new(-22.0, 0.0));
        let record_rect = mute_rect.translate(Vec2::new(-44.0, 0.0));
        painter.rect_filled(record_rect, CornerRadius::same(3), theme::GRID);
        painter.text(
            record_rect.center(),
            Align2::CENTER_CENTER,
            "R",
            FontId::monospace(9.5),
            theme::TEXT_MUTED,
        );
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
            FontId::monospace(9.5),
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
            FontId::monospace(9.5),
            if track.mixer.solo {
                Color32::BLACK
            } else {
                theme::TEXT_MUTED
            },
        );

        let icon_rect = Rect::from_min_size(
            egui::pos2(content_left + 1.0, header_rect.top() + 38.0),
            Vec2::splat(16.0),
        );
        draw_track_icon(&painter, icon_rect, track.kind, track_color);
        let menu_rect = Rect::from_center_size(
            egui::pos2(content_right - 9.0, header_rect.top() + 45.0),
            Vec2::new(18.0, 18.0),
        );
        painter.rect_filled(menu_rect, CornerRadius::same(3), theme::GRID);
        crate::icons::paint(
            &painter,
            menu_rect,
            crate::icons::Icon::Menu,
            theme::TEXT_MUTED,
        );
        let fader_rect = Rect::from_min_max(
            egui::pos2(icon_rect.right() + 8.0, header_rect.top() + 38.0),
            egui::pos2(menu_rect.left() - 6.0, header_rect.top() + 53.0),
        );
        if let Some(volume) = crate::widgets::volume_fader(
            ui,
            Id::new(("track-volume", track_id)),
            fader_rect,
            track.mixer.volume_db.0,
            crate::widgets::DB_MIN,
            crate::widgets::DB_MAX,
        ) {
            self.emit(UiIntent::SetTrackMixer {
                track_id,
                volume_db: Some(notwig_core::Db(volume)),
                pan: None,
                mute: None,
                solo: None,
            });
        }

        let meter_rect = Rect::from_min_max(
            egui::pos2(header_rect.right() - meter_w - 3.0, header_rect.top() + 4.0),
            egui::pos2(header_rect.right() - 3.0, header_rect.bottom() - 4.0),
        );
        let meter = runtime.meters.get(&track_id).copied().unwrap_or_default();
        draw_vertical_meter(&painter, meter_rect, meter.peak_l, meter.peak_r);

        let header_resp = ui.interact(
            header_rect,
            Id::new(("track-header", track_id)),
            Sense::click(),
        );
        if header_resp.clicked() {
            let additive = ui.input(|i| i.modifiers.command || i.modifiers.shift);
            self.state.selection.select_track(track_id, additive);
            self.emit(UiIntent::SelectTrack(track_id));
        }
        let header_double_click = ui.input(|input| {
            input
                .pointer
                .button_double_clicked(egui::PointerButton::Primary)
                .then(|| input.pointer.interact_pos())
                .flatten()
        });
        if let Some(pos) = header_double_click
            && header_rect.contains(pos)
            && ![
                mute_rect,
                solo_rect,
                record_rect,
                icon_rect,
                fader_rect,
                menu_rect,
                meter_rect,
            ]
            .iter()
            .any(|control| control.contains(pos))
        {
            self.state.selection.select_track(track_id, false);
            self.emit(UiIntent::SelectTrack(track_id));
            self.open_device_editor();
        }
        if track.kind != TrackKind::Master {
            let drag_rect = Rect::from_min_max(
                egui::pos2(content_left, header_rect.top() + 2.0),
                egui::pos2(content_right - 68.0, header_rect.top() + 28.0),
            );
            let drag_resp =
                ui.interact(drag_rect, Id::new(("track-drag", track_id)), Sense::drag());
            if drag_resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
            if drag_resp.drag_started_by(egui::PointerButton::Primary) {
                self.track_drag = Some(track_id);
                self.track_drop = None;
                self.state.selection.select_track(track_id, false);
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }
            if drag_resp.dragged_by(egui::PointerButton::Primary)
                && self.track_drag == Some(track_id)
            {
                if let Some(pointer) = ui.input(|i| i.pointer.hover_pos()) {
                    self.track_drop = self.track_drop_target(doc, track_id, pointer);
                }
            }
            if drag_resp.drag_stopped_by(egui::PointerButton::Primary)
                && let Some(dragged) = self.track_drag.take()
            {
                if let Some(drop) = self.track_drop.take() {
                    self.emit(UiIntent::MoveTrack {
                        id: dragged,
                        parent: drop.parent,
                        index: drop.index,
                    });
                }
            }
        }
        let selected_tracks: Vec<TrackId> = self.state.selection.tracks.iter().copied().collect();
        header_resp.context_menu(|ui| {
            if ui.button("Group selected tracks").clicked() {
                let tracks = if selected_tracks.is_empty() {
                    vec![track_id]
                } else {
                    selected_tracks.clone()
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
            if track.kind != TrackKind::Master && ui.button("Delete track").clicked() {
                self.emit(UiIntent::DeleteTrack(track_id));
                ui.close();
            }
        });
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
                ui.input(|i| i.modifiers.shift),
            );
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

        if let Some(time) = &self.state.selection.time
            && time.contains_track(track_id)
        {
            crate::timeline::draw_time_selection_fill(
                &painter,
                TimelineGrid::new(start, ppb, doc.meta.time_sig_num.max(1)),
                lane_rect.left(),
                lane_rect,
                time.range,
            );
        }
    }

    fn track_drop_target(
        &self,
        doc: &ProjectDoc,
        dragged: TrackId,
        pointer: egui::Pos2,
    ) -> Option<TrackDrop> {
        let descendants = doc.track_order.descendants(dragged);
        let valid_parent = |parent: Option<TrackId>| {
            parent.is_none_or(|parent| parent != dragged && !descendants.contains(&parent))
        };
        let master = doc.master_track();
        let mut rows: Vec<(TrackId, Rect)> = doc
            .track_order
            .depth_first()
            .into_iter()
            .filter(|id| Some(*id) != master)
            .filter_map(|id| self.row_rect(id).map(|row| (id, row)))
            .collect();
        rows.sort_by(|left, right| left.1.top().total_cmp(&right.1.top()));
        let first_below = rows.iter().position(|(_, row)| pointer.y < row.center().y);
        let (above, below, y) = if let Some(index) = first_below {
            let below = rows[index];
            (
                index.checked_sub(1).map(|previous| rows[previous]),
                Some(below),
                below.1.top(),
            )
        } else {
            let above = *rows.last()?;
            (Some(above), None, above.1.bottom())
        };
        let current_parent = doc.tracks.get(&dragged)?.parent;
        let current_index = Self::sibling_index(doc, current_parent, dragged);
        let mut candidates: Vec<(Option<TrackId>, usize)> = Vec::new();
        let mut add_candidate =
            |parent: Option<TrackId>, reference: Option<TrackId>, after: bool| {
                if !valid_parent(parent) {
                    return;
                }
                if let Some(index) = Self::insertion_index(doc, parent, reference, after, dragged) {
                    let candidate = (parent, index);
                    if !candidates.contains(&candidate) {
                        candidates.push(candidate);
                    }
                }
            };
        if let Some((below_id, _)) = below {
            add_candidate(doc.tracks.get(&below_id)?.parent, Some(below_id), false);
        }
        if let Some((above_id, _)) = above {
            let ends_here = below.is_none_or(|(below_id, _)| {
                !doc.track_order.descendants(above_id).contains(&below_id)
            });
            if ends_here {
                let mut current = above_id;
                loop {
                    let parent = doc.tracks.get(&current)?.parent;
                    add_candidate(parent, Some(current), true);
                    let Some(parent_id) = parent else {
                        break;
                    };
                    if Self::siblings(doc, Some(parent_id)).last() != Some(&current) {
                        break;
                    }
                    current = parent_id;
                }
            }
            let above_is_empty_group = doc
                .tracks
                .get(&above_id)
                .is_some_and(|track| track.kind.can_have_children())
                && doc.track_order.children_of(above_id).is_empty();
            if above_is_empty_group && valid_parent(Some(above_id)) {
                add_candidate(Some(above_id), None, true);
            }
        }
        let base_x = rows.first()?.1.left() + 4.0;
        let mut fallback: Option<(Option<TrackId>, usize, usize)> = None;
        let mut chosen: Option<(Option<TrackId>, usize, usize)> = None;
        for (parent, index) in candidates {
            let depth = parent.map_or(0, |parent| track_depth(doc, parent) + 1);
            let candidate = (parent, index, depth);
            if fallback.is_none_or(|(_, _, best)| depth < best) {
                fallback = Some(candidate);
            }
            if base_x + depth as f32 * TRACK_INDENT <= pointer.x + 0.5
                && chosen.is_none_or(|(_, _, best)| depth > best)
            {
                chosen = Some(candidate);
            }
        }
        let (parent, index, depth) = chosen.or(fallback)?;
        if parent == current_parent && index == current_index {
            return None;
        }
        Some(TrackDrop {
            parent,
            index,
            y,
            depth,
        })
    }

    fn siblings(doc: &ProjectDoc, parent: Option<TrackId>) -> &[TrackId] {
        match parent {
            Some(parent) => doc.track_order.children_of(parent),
            None => &doc.track_order.roots,
        }
    }

    fn insertion_index(
        doc: &ProjectDoc,
        parent: Option<TrackId>,
        reference: Option<TrackId>,
        after: bool,
        dragged: TrackId,
    ) -> Option<usize> {
        let siblings = Self::siblings(doc, parent);
        let dragged_index = siblings.iter().position(|id| *id == dragged);
        match reference {
            Some(reference) => {
                if reference == dragged {
                    dragged_index
                } else {
                    let mut index = siblings.iter().position(|id| *id == reference)?;
                    if dragged_index.is_some_and(|dragged| dragged < index) {
                        index -= 1;
                    }
                    Some(if after { index + 1 } else { index })
                }
            }
            None => Some(siblings.len() - usize::from(dragged_index.is_some())),
        }
    }

    fn sibling_index(doc: &ProjectDoc, parent: Option<TrackId>, id: TrackId) -> usize {
        let siblings = Self::siblings(doc, parent);
        siblings
            .iter()
            .position(|sibling| *sibling == id)
            .unwrap_or(siblings.len())
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

    fn clip_context_menu(
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

fn draw_track_prefixes(
    painter: &egui::Painter,
    doc: &ProjectDoc,
    track: &notwig_core::tracks::Track,
    row_rect: Rect,
    depth: f32,
) {
    let mut ancestor = track.parent;
    let mut level = depth as usize;
    while let Some(parent_id) = ancestor {
        if level == 0 {
            break;
        }
        level -= 1;
        let Some(parent) = doc.tracks.get(&parent_id) else {
            break;
        };
        let color = parent.color.map_or_else(
            || track_color(parent.kind),
            |color| Color32::from_rgba_unmultiplied(color.r, color.g, color.b, 255),
        );
        let x = row_rect.left() + level as f32 * 14.0 + 2.0;
        painter.line_segment(
            [
                egui::pos2(x, row_rect.top() + 1.0),
                egui::pos2(x, row_rect.bottom() - 1.0),
            ],
            Stroke::new(3.0_f32, color),
        );
        ancestor = parent.parent;
    }
}

fn draw_vertical_meter(painter: &egui::Painter, rect: Rect, peak_l: f32, peak_r: f32) {
    crate::widgets::draw_stereo_meter(painter, rect, peak_l, peak_r);
}

fn draw_track_icon(painter: &egui::Painter, rect: Rect, kind: TrackKind, color: Color32) {
    let icon = match kind {
        TrackKind::Group => crate::icons::Icon::Folder,
        TrackKind::Audio => crate::icons::Icon::Audio,
        TrackKind::Instrument => crate::icons::Icon::Instrument,
        TrackKind::BusFx | TrackKind::Master => crate::icons::Icon::Device,
    };
    crate::icons::paint(painter, rect, icon, color);
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
