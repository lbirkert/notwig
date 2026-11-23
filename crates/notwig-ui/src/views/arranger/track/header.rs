use egui::{Align2, Color32, CornerRadius, FontId, Id, Rect, Sense, Stroke, Vec2};
use notwig_core::tracks::Track;
use notwig_core::{ProjectDoc, TrackId, TrackKind};

use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::shell::NotwigUi;
use crate::theme;

#[derive(Clone, Copy)]
pub(super) struct HeaderLayout {
    pub(super) row_rect: Rect,
    pub(super) header_rect: Rect,
    pub(super) depth: f32,
    pub(super) has_children: bool,
    pub(super) twisty_rect: Rect,
    pub(super) name_left: f32,
    pub(super) name_rect: Rect,
    pub(super) mute_rect: Rect,
    pub(super) solo_rect: Rect,
    pub(super) record_rect: Rect,
    pub(super) icon_rect: Rect,
    pub(super) menu_rect: Rect,
    pub(super) fader_rect: Rect,
    pub(super) meter_rect: Rect,
    pub(super) drag_rect: Option<Rect>,
}

impl HeaderLayout {
    pub(super) fn new(
        doc: &ProjectDoc,
        track: &Track,
        track_id: TrackId,
        row_rect: Rect,
        header_rect: Rect,
        depth: f32,
    ) -> Self {
        let has_children = !doc.track_order.children_of(track_id).is_empty();
        let meter_w = 12.0;
        let content_left = header_rect.left() + 10.0;
        let content_right = header_rect.right() - meter_w - 8.0;
        let twisty_rect = Rect::from_min_size(
            egui::pos2(content_left - 1.0, header_rect.top() + 6.0),
            Vec2::splat(14.0),
        );
        let name_left = content_left + if has_children { 14.0 } else { 0.0 };
        let name_rect = Rect::from_min_max(
            egui::pos2(name_left, header_rect.top() + 4.0),
            egui::pos2(content_right - 68.0, header_rect.top() + 26.0),
        );
        let mute_rect = Rect::from_center_size(
            egui::pos2(content_right - 9.0, header_rect.top() + 16.0),
            Vec2::splat(18.0),
        );
        let solo_rect = mute_rect.translate(Vec2::new(-22.0, 0.0));
        let record_rect = mute_rect.translate(Vec2::new(-44.0, 0.0));
        let icon_rect = Rect::from_min_size(
            egui::pos2(content_left + 1.0, header_rect.top() + 38.0),
            Vec2::splat(16.0),
        );
        let menu_rect = Rect::from_center_size(
            egui::pos2(content_right - 9.0, header_rect.top() + 45.0),
            Vec2::new(18.0, 18.0),
        );
        let fader_rect = Rect::from_min_max(
            egui::pos2(icon_rect.right() + 8.0, header_rect.top() + 38.0),
            egui::pos2(menu_rect.left() - 6.0, header_rect.top() + 53.0),
        );
        let meter_rect = Rect::from_min_max(
            egui::pos2(header_rect.right() - meter_w - 3.0, header_rect.top() + 4.0),
            egui::pos2(header_rect.right() - 3.0, header_rect.bottom() - 4.0),
        );
        let drag_rect = (track.kind != TrackKind::Master).then(|| {
            let left = content_left + if has_children { 14.0 } else { 0.0 };
            Rect::from_min_max(
                egui::pos2(left, header_rect.top() + 2.0),
                egui::pos2(content_right - 68.0, header_rect.top() + 28.0),
            )
        });
        Self {
            row_rect,
            header_rect,
            depth,
            has_children,
            twisty_rect,
            name_left,
            name_rect,
            mute_rect,
            solo_rect,
            record_rect,
            icon_rect,
            menu_rect,
            fader_rect,
            meter_rect,
            drag_rect,
        }
    }
}

impl NotwigUi {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_track_header(
        &mut self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        doc: &ProjectDoc,
        track: &Track,
        track_id: TrackId,
        layout: &HeaderLayout,
        selected_track: bool,
        runtime: &RuntimeView,
    ) {
        let HeaderLayout {
            row_rect,
            header_rect,
            depth,
            has_children,
            twisty_rect,
            name_left,
            name_rect,
            icon_rect,
            menu_rect,
            fader_rect,
            meter_rect,
            ..
        } = *layout;
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
        draw_track_prefixes(painter, doc, track, row_rect, depth);
        if has_children {
            let collapsed = self.collapsed_tracks.contains(&track_id);
            let center = twisty_rect.center();
            let points = if collapsed {
                vec![
                    egui::pos2(center.x - 2.5, center.y - 4.0),
                    egui::pos2(center.x - 2.5, center.y + 4.0),
                    egui::pos2(center.x + 3.5, center.y),
                ]
            } else {
                vec![
                    egui::pos2(center.x - 4.0, center.y - 2.5),
                    egui::pos2(center.x + 4.0, center.y - 2.5),
                    egui::pos2(center.x, center.y + 3.5),
                ]
            };
            painter.add(egui::Shape::convex_polygon(
                points,
                theme::TEXT_MUTED,
                Stroke::NONE,
            ));
        }
        let name_color = if selected_track {
            Color32::WHITE
        } else {
            theme::TEXT_MUTED
        };
        painter.with_clip_rect(name_rect).text(
            egui::pos2(name_left, header_rect.top() + 6.0),
            Align2::LEFT_TOP,
            &track.name,
            FontId::proportional(13.0),
            name_color,
        );

        let mute_rect = layout.mute_rect;
        let solo_rect = layout.solo_rect;
        let record_rect = layout.record_rect;
        painter.rect_filled(
            record_rect,
            CornerRadius::same(3),
            if track.record_armed {
                Color32::from_rgb(150, 48, 48)
            } else {
                theme::GRID
            },
        );
        painter.text(
            record_rect.center(),
            Align2::CENTER_CENTER,
            "R",
            FontId::monospace(9.5),
            if track.record_armed {
                Color32::WHITE
            } else {
                theme::TEXT_MUTED
            },
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

        draw_track_icon(painter, icon_rect, track.kind, track_color);
        painter.rect_filled(menu_rect, CornerRadius::same(3), theme::GRID);
        crate::icons::paint(
            painter,
            menu_rect,
            crate::icons::Icon::Menu,
            theme::TEXT_MUTED,
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

        let meter = runtime.meters.get(&track_id).copied().unwrap_or_default();
        draw_vertical_meter(painter, meter_rect, meter.peak_l, meter.peak_r);
    }

    pub(super) fn handle_track_header(
        &mut self,
        ui: &mut egui::Ui,
        doc: &ProjectDoc,
        track: &Track,
        track_id: TrackId,
        layout: &HeaderLayout,
    ) {
        let HeaderLayout {
            header_rect,
            has_children,
            twisty_rect,
            mute_rect,
            solo_rect,
            record_rect,
            icon_rect,
            fader_rect,
            menu_rect,
            meter_rect,
            drag_rect,
            ..
        } = *layout;
        let header_resp = ui.interact(
            header_rect,
            Id::new(("track-header", track_id)),
            Sense::click(),
        );
        let twisty_resp = has_children.then(|| {
            ui.interact(
                twisty_rect,
                Id::new(("track-twisty", track_id)),
                Sense::click(),
            )
        });
        let twisty_clicked = twisty_resp.is_some_and(|response| response.clicked());
        if twisty_clicked && !self.collapsed_tracks.remove(&track_id) {
            self.collapsed_tracks.insert(track_id);
        }
        let pointer_in_drag_handle = drag_rect.is_some_and(|rect| {
            header_resp
                .interact_pointer_pos()
                .is_some_and(|pos| rect.contains(pos))
        });
        if header_resp.clicked() && !twisty_clicked && !pointer_in_drag_handle {
            let command = ui.input(|i| i.modifiers.command);
            let shift = ui.input(|i| i.modifiers.shift);
            self.select_track_from_click(doc, track_id, shift, command);
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
        let selected_tracks: Vec<TrackId> = self.state.selection.tracks.iter().copied().collect();
        if let Some(drag_rect) = drag_rect {
            let drag_resp = ui.interact(
                drag_rect,
                Id::new(("track-drag", track_id)),
                Sense::click_and_drag(),
            );
            drag_resp.context_menu(|ui| {
                self.track_header_menu(ui, track_id, track.kind, &selected_tracks);
            });
            let on_twisty = |response: &egui::Response| {
                has_children
                    && response
                        .interact_pointer_pos()
                        .is_some_and(|pos| twisty_rect.contains(pos))
            };
            if drag_resp.clicked() && !on_twisty(&drag_resp) {
                let command = ui.input(|i| i.modifiers.command);
                let shift = ui.input(|i| i.modifiers.shift);
                self.select_track_from_click(doc, track_id, shift, command);
            }
            if drag_resp.dragged_by(egui::PointerButton::Primary)
                || self.track_drag == Some(track_id)
            {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }
            if drag_resp.drag_started_by(egui::PointerButton::Primary) && !on_twisty(&drag_resp) {
                let command = ui.input(|i| i.modifiers.command);
                let shift = ui.input(|i| i.modifiers.shift);
                if shift || command {
                    self.select_track_from_click(doc, track_id, shift, command);
                } else {
                    self.track_drag = Some(track_id);
                    self.track_drop = None;
                    self.state.selection.select_track(track_id, false);
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                }
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
        header_resp.context_menu(|ui| {
            self.track_header_menu(ui, track_id, track.kind, &selected_tracks);
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
        if track.kind == TrackKind::Audio
            && ui
                .interact(
                    record_rect,
                    Id::new(("record-arm", track_id)),
                    Sense::click(),
                )
                .on_hover_text(if track.record_armed {
                    "Disarm recording"
                } else {
                    "Arm for recording"
                })
                .clicked()
        {
            self.emit(UiIntent::SetTrackRecordArm {
                track_id,
                armed: !track.record_armed,
            });
        }
    }

    fn select_track_from_click(
        &mut self,
        doc: &ProjectDoc,
        track_id: TrackId,
        shift: bool,
        command: bool,
    ) {
        if shift {
            let anchor = self.state.selection.tracks.primary().unwrap_or(track_id);
            let order = doc.track_order.depth_first();
            self.state
                .selection
                .select_track_range(anchor, track_id, &order);
        } else if command {
            self.state.selection.add_track(track_id);
        } else {
            self.state.selection.select_track(track_id, false);
        }
        self.emit(UiIntent::SelectTrack(track_id));
    }
}

fn draw_track_prefixes(
    painter: &egui::Painter,
    doc: &ProjectDoc,
    track: &Track,
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
        let x = row_rect.left() + level as f32 * 14.0;
        painter.rect_filled(
            Rect::from_min_size(
                egui::pos2(x, row_rect.top()),
                Vec2::new(4.0, row_rect.height()),
            ),
            CornerRadius::ZERO,
            color,
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
