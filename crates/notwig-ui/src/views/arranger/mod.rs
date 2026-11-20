use egui::{Align2, Color32, CornerRadius, FontId, Id, Rect, Sense, Stroke, Vec2};
use notwig_core::{ClipId, ProjectDoc, RangeMus, TimeMus, TrackId, TrackKind};

pub(crate) mod edit;

use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::shell::NotwigUi;
use crate::theme;
use crate::timeline::TimelineGrid;
use crate::widgets::snap_step;

const HEADER_W: f32 = 230.0;
const ROW_H: f32 = 64.0;
const RULER_H: f32 = 38.0;
const SCROLLBAR_H: f32 = 14.0;
pub(crate) const TRACK_INDENT: f32 = 14.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackDrop {
    pub parent: Option<TrackId>,
    pub index: usize,
    pub y: f32,
    pub depth: usize,
}

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

#[derive(Debug, Clone, PartialEq)]
pub struct GroupDrag {
    pub start_pointer: egui::Pos2,
    pub primary_origin: RangeMus,
    pub origins: Vec<(ClipId, TrackId, RangeMus)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BarSelectionDrag {
    pub start_pointer: egui::Pos2,
    pub origin: RangeMus,
    pub tracks: Vec<TrackId>,
}

impl NotwigUi {
    pub(crate) fn arranger_view(
        &mut self,
        ui: &mut egui::Ui,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
    ) {
        if ui.rect_contains_pointer(ui.max_rect()) && ui.input(|input| input.pointer.any_pressed())
        {
            self.edit_focus = crate::state::EditFocus::Timeline;
        }
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

        let (ruler_rect, _) = ui.allocate_exact_size(Vec2::new(width, RULER_H), Sense::hover());
        let timeline_left = ruler_rect.left() + header_w;
        self.timeline_left = timeline_left;
        let playhead = runtime.playing.then_some(runtime.playhead);
        let strip = crate::timeline::BeatStrip {
            grid: TimelineGrid::new(start, ppb, beats_per_bar),
            header_width: header_w,
            header_label: Some("TRACKS"),
            loop_range: if doc.transport.loop_enabled {
                doc.transport.loop_range
            } else {
                None
            },
            time_selection: self.state.selection.time.as_ref().map(|time| time.range),
            play_start: runtime.play_start,
            playhead: playhead.unwrap_or(start - TimeMus::new(1_000_000, 1)),
            show_play_start: true,
            loop_bottom_inset: 14.0,
        };
        if let Some(action) = strip.show(ui, ruler_rect, Id::new("arranger-ruler")) {
            match action {
                crate::timeline::BeatStripAction::Locate(beat) => {
                    self.emit(UiIntent::SetPlayStart(beat));
                }
                crate::timeline::BeatStripAction::SetLoop(range) => {
                    self.emit(UiIntent::SetLoopRange(range));
                }
            }
        }

        let rows_top = ui.cursor().top();
        let master = doc.master_track();
        let master_h = if master.is_some() { ROW_H } else { 0.0 };
        let rows_height = (ui.available_height() - SCROLLBAR_H - 4.0).max(60.0);
        let tree_ids: Vec<TrackId> = visible_tracks(doc, &self.collapsed_tracks)
            .into_iter()
            .filter(|id| Some(*id) != master)
            .collect();
        let add_row_h = 28.0;
        let tree_content_h = tree_ids.len() as f32 * ROW_H + add_row_h;
        let tree_min_h = (rows_height - master_h).max(tree_content_h);
        let tree_rect = Rect::from_min_max(
            egui::pos2(ruler_rect.left(), rows_top),
            egui::pos2(timeline_left, rows_top + rows_height),
        );
        let content_bg = Rect::from_min_max(
            egui::pos2(timeline_left, rows_top),
            egui::pos2(ruler_rect.right(), rows_top + rows_height),
        );
        let background = ui.painter();
        background.rect_filled(tree_rect, CornerRadius::ZERO, Color32::BLACK);
        background.rect_filled(content_bg, CornerRadius::ZERO, theme::LANE);
        TimelineGrid::new(start, ppb, beats_per_bar).draw(background, content_bg);
        ui.allocate_ui_with_layout(
            Vec2::new(width, rows_height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("arranger_rows")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.allocate_ui_with_layout(
                            Vec2::new(width, tree_min_h),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                for track_id in &tree_ids {
                                    self.track_row(
                                        ui, doc, *track_id, header_w, ppb, start, runtime, grid,
                                    );
                                }
                                let (add_rect, add_resp) = ui.allocate_exact_size(
                                    Vec2::new(header_w, add_row_h),
                                    Sense::click(),
                                );
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
                                add_resp.context_menu(|ui| {
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
                                });
                            },
                        );
                        if let Some(master) = master {
                            self.track_row(ui, doc, master, header_w, ppb, start, runtime, grid);
                        }
                    });
            },
        );

        if self.track_drag.is_some()
            && let Some(drop) = self.track_drop
        {
            let rows_rect = Rect::from_min_max(
                egui::pos2(ruler_rect.left(), rows_top),
                egui::pos2(ruler_rect.right(), rows_top + rows_height),
            );
            let painter = ui
                .ctx()
                .layer_painter(egui::LayerId::new(
                    egui::Order::Foreground,
                    Id::new("track-drop-line"),
                ))
                .with_clip_rect(rows_rect);
            let line_x = rows_rect.left() + 4.0 + drop.depth as f32 * TRACK_INDENT;
            painter.line_segment(
                [
                    egui::pos2(line_x, drop.y),
                    egui::pos2(rows_rect.right(), drop.y),
                ],
                Stroke::new(2.0_f32, Color32::WHITE),
            );
            painter.circle_filled(egui::pos2(line_x, drop.y), 3.0, Color32::WHITE);
        }

        self.bar_selection_interaction(ui, doc, ppb, start, grid);
        let content_rect = Rect::from_min_max(
            egui::pos2(timeline_left, rows_top),
            egui::pos2(ruler_rect.right(), rows_top + rows_height),
        );
        let overlay = ui.painter().with_clip_rect(content_rect);
        if let Some(playhead) = playhead {
            crate::timeline::draw_playhead(
                &overlay,
                content_rect,
                timeline_left,
                TimelineGrid::new(start, ppb, beats_per_bar),
                playhead,
            );
        }
        if let Some(time) = &self.state.selection.time {
            let marker_rect = Rect::from_min_max(
                egui::pos2(timeline_left, ruler_rect.top()),
                egui::pos2(content_rect.right(), content_rect.bottom()),
            );
            let selection_painter = ui.painter().with_clip_rect(marker_rect);
            crate::timeline::draw_time_selection_markers(
                &selection_painter,
                TimelineGrid::new(start, ppb, beats_per_bar),
                timeline_left,
                marker_rect,
                ruler_rect,
                time.range,
            );
        }
        let (bar_rect, _) = ui.allocate_exact_size(Vec2::new(width, SCROLLBAR_H), Sense::hover());
        self.draw_drop_preview(ui, doc, header_w);
        self.timeline_scrollbar(ui, bar_rect, doc, header_w, ppb);
    }
}

mod clip;
mod navigation;
mod preview;
mod selection;
mod track;

fn visible_tracks(
    doc: &ProjectDoc,
    collapsed: &std::collections::BTreeSet<TrackId>,
) -> Vec<TrackId> {
    fn visit(
        doc: &ProjectDoc,
        collapsed: &std::collections::BTreeSet<TrackId>,
        id: TrackId,
        out: &mut Vec<TrackId>,
    ) {
        out.push(id);
        if collapsed.contains(&id) {
            return;
        }
        for child in doc.track_order.children_of(id) {
            visit(doc, collapsed, *child, out);
        }
    }

    let mut out = Vec::new();
    for root in &doc.track_order.roots {
        visit(doc, collapsed, *root, &mut out);
    }
    out
}
