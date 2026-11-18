use egui::{Align2, Color32, CornerRadius, FontId, Id, Rect, Sense, Stroke, StrokeKind, Vec2};
use notwig_core::edit::{
    self, EditError, EditModel, EditOp, EditorKind, Extract, LaneBounds, NextSelection,
    ObjectExtent, ObjectKind, ObjectView, Plan, Selection, TrimLimits, epsilon,
};
use notwig_core::sequencer::{AudioClipData, AudioSlice};
use notwig_core::{ClipId, ClipPayload, Db, ProjectDoc, RangeMus, SliceId, TempoMap, TimeMus};
use uuid::Uuid;

use crate::edit::paste_target;
use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::shell::NotwigUi;
use crate::state::{SelectionGroup, Viewport};
use crate::theme;
use crate::timeline::TimelineGrid;
use crate::widgets::{snap_step, snap_time};

const MICRO: u32 = 1_000_000;
const CLIP_HEADER_W: f32 = 120.0;
const RULER_H: f32 = 30.0;
const SCROLLBAR_H: f32 = 14.0;

#[derive(Debug, Clone)]
pub struct ClipEditorState {
    pub clip_id: ClipId,
    pub viewport: Viewport,
    selected: SelectionGroup<SliceId>,
    bar_selection: Option<RangeMus>,
    clipboard: SliceClipboard,
    drag: Option<SliceDrag>,
    edge_drag: Option<ClipEdgeDrag>,
    preview_range: Option<RangeMus>,
    range_start: Option<TimeMus>,
    bar_drag: Option<BarDrag>,
    initialized: bool,
}

#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliceDragMode {
    Move,
    TrimStart,
    TrimEnd,
}

#[doc(hidden)]
#[derive(Debug, Clone, Copy)]
pub struct SliceDrag {
    slice_id: SliceId,
    mode: SliceDragMode,
    origin_pointer: egui::Pos2,
    origin_start: TimeMus,
    origin_source_start: u64,
    origin_source_end: u64,
}

#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipEdge {
    Start,
    End,
}

#[doc(hidden)]
#[derive(Debug, Clone, Copy)]
pub struct ClipEdgeDrag {
    edge: ClipEdge,
    start_pointer: egui::Pos2,
    origin: RangeMus,
}

#[derive(Debug, Clone)]
struct BarDrag {
    start_pointer: egui::Pos2,
    origin: RangeMus,
    slices: Vec<SliceId>,
}

#[derive(Debug, Clone, Default)]
struct SliceClipboard {
    extract: Option<Extract<ClipLane, AudioSlice>>,
}

impl ClipEditorState {
    pub fn new(clip_id: ClipId) -> Self {
        Self {
            clip_id,
            viewport: Viewport {
                start: TimeMus::zero(),
                pixels_per_beat: 80.0,
                scroll_track: 0,
            },
            selected: SelectionGroup::default(),
            bar_selection: None,
            clipboard: SliceClipboard::default(),
            drag: None,
            edge_drag: None,
            preview_range: None,
            range_start: None,
            bar_drag: None,
            initialized: false,
        }
    }

    #[doc(hidden)]
    pub fn select_node(&mut self, id: SliceId, additive: bool) {
        if additive {
            self.selected.toggle(id);
        } else {
            self.selected.clear();
            self.bar_selection = None;
            self.selected.select_only(id);
        }
    }

    #[doc(hidden)]
    pub fn select_bar(&mut self, range: RangeMus) {
        self.selected.clear();
        self.bar_selection = Some(range);
    }

    #[doc(hidden)]
    pub fn selection_mode(&self) -> Option<notwig_core::edit::SelectionMode> {
        let selection = if self.selected.is_empty() {
            self.bar_selection.map_or_else(Selection::default, |range| {
                Selection::time(range, [ClipLane])
            })
        } else {
            Selection::objects(self.selected.iter().copied())
        };
        selection.mode()
    }

    #[doc(hidden)]
    pub fn selected_is_empty(&self) -> bool {
        self.selected.is_empty()
    }

    #[doc(hidden)]
    pub fn selected_contains(&self, id: SliceId) -> bool {
        self.selected.contains(&id)
    }

    #[doc(hidden)]
    pub fn bar_selection(&self) -> Option<RangeMus> {
        self.bar_selection
    }
}

impl NotwigUi {
    pub(crate) fn clip_editor_panel(
        &mut self,
        ctx: &egui::Context,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
    ) {
        let Some(mut state) = self.clip_editor.clone() else {
            self.clip_editor_placeholder(ctx);
            return;
        };
        let Some(clip) = doc.arranger.clips.get(&state.clip_id).cloned() else {
            self.close_clip_editor();
            return;
        };
        let ClipPayload::Audio(audio) = &clip.payload else {
            self.close_clip_editor();
            return;
        };
        let mut audio = audio.clone();
        let map = doc.tempo_map();
        if !state.initialized {
            state.viewport.start = (clip.range.start - TimeMus::new(4, 1)).max(TimeMus::zero());
            state.initialized = true;
        }
        let clip_start = clip.range.start;
        let display_range = state.preview_range.unwrap_or(clip.range);

        handle_clip_editor_actions(
            self,
            ctx,
            &mut state,
            &mut audio,
            &clip,
            &map,
            runtime.playhead,
        );

        let open = super::bottom_panel(
            ctx,
            super::PanelConfig {
                id: "clip_editor_panel",
                resizable: true,
                default_height: 420.0,
                min_height: 260.0,
                max_height: 700.0,
                handle_id: "clip-editor-panel",
                handle_tooltip: "Drag down to close",
            },
            |ui| {
                let width = ui.available_width().max(320.0);
                let height = (ui.available_height() - SCROLLBAR_H).max(120.0);
                let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
                let header = Rect::from_min_max(
                    rect.min,
                    egui::pos2(rect.left() + CLIP_HEADER_W, rect.bottom()),
                );
                let content = Rect::from_min_max(egui::pos2(header.right(), rect.top()), rect.max);
                let ruler = Rect::from_min_size(content.min, Vec2::new(content.width(), RULER_H));
                let lanes =
                    Rect::from_min_max(egui::pos2(content.left(), ruler.bottom()), content.max);
                self.clip_editor_content_rect = content;
                if ui.rect_contains_pointer(rect) && ui.input(|input| input.pointer.any_pressed()) {
                    self.edit_focus = crate::state::EditFocus::ClipEditor;
                }

                clip_editor_navigation(&mut state, ui, content);

                let ppb = state
                    .viewport
                    .pixels_per_beat
                    .clamp(Viewport::MIN_PPB, Viewport::MAX_PPB);
                let beats_per_bar = doc.meta.time_sig_num.max(1);
                let grid = snap_step(ppb, beats_per_bar);
                let timeline_grid = TimelineGrid::new(state.viewport.start, ppb, beats_per_bar);
                let clip_start_x = timeline_grid.x_for_beat(display_range.start, content.left());
                let clip_end_x = timeline_grid.x_for_beat(display_range.end, content.left());

                let painter = ui.painter().clone();
                painter.rect_filled(rect, CornerRadius::ZERO, Color32::BLACK);
                painter.rect_filled(header, CornerRadius::ZERO, theme::PANEL_LIGHT);
                painter.rect_filled(lanes, CornerRadius::ZERO, theme::LANE);

                let strip_rect =
                    Rect::from_min_max(rect.min, egui::pos2(content.right(), rect.top() + RULER_H));
                let strip = crate::timeline::BeatStrip {
                    grid: timeline_grid,
                    header_width: CLIP_HEADER_W,
                    header_label: Some("AUDIO EVENTS"),
                    loop_range: if doc.transport.loop_enabled {
                        doc.transport.loop_range
                    } else {
                        None
                    },
                    time_selection: state.bar_selection,
                    play_start: runtime.play_start,
                    playhead: runtime.playhead,
                    show_play_start: true,
                };
                if let Some(beat) = strip.show(
                    ui,
                    strip_rect,
                    Id::new(("clip-editor-ruler", state.clip_id)),
                ) {
                    self.emit(UiIntent::SetPlayStart(beat));
                }
                let clipped = painter.with_clip_rect(content);

                let bg_resp = ui.interact(
                    lanes,
                    Id::new(("clip-editor-bg", state.clip_id)),
                    Sense::click_and_drag(),
                );
                if bg_resp.clicked() {
                    state.selected.clear();
                    state.bar_selection = None;
                }
                if bg_resp.drag_started() {
                    state.selected.clear();
                    state.bar_selection = None;
                    if let Some(pos) = ui
                        .input(|input| input.pointer.press_origin())
                        .or_else(|| bg_resp.interact_pointer_pos())
                    {
                        let bypass = ui.input(|input| input.modifiers.shift);
                        let beat = timeline_grid
                            .snap(timeline_grid.beat_for_x(pos.x, content.left()), bypass)
                            .max(TimeMus::zero());
                        state.range_start = Some(beat);
                    }
                }
                if bg_resp.dragged() {
                    if let (Some(origin), Some(pos)) =
                        (state.range_start, bg_resp.interact_pointer_pos())
                    {
                        let bypass = ui.input(|input| input.modifiers.shift);
                        let beat = timeline_grid
                            .snap(timeline_grid.beat_for_x(pos.x, content.left()), bypass)
                            .max(TimeMus::zero());
                        if let Some(range) =
                            crate::timeline::interaction::selection_range(origin, beat)
                        {
                            state.select_bar(range);
                        }
                    }
                }
                if bg_resp.drag_stopped() {
                    state.range_start = None;
                }

                let bypass = ui.input(|input| input.modifiers.alt);
                let slices = editor_slices(&clip, &audio);
                let drag_pointer = ui
                    .input(|input| input.pointer.hover_pos())
                    .or_else(|| state.drag.map(|drag| drag.origin_pointer));
                let drag_delta = state.drag.map_or(0.0, |drag| {
                    drag_pointer.map_or(0.0, |pointer| {
                        f64::from(pointer.x - drag.origin_pointer.x) / f64::from(ppb)
                    })
                });
                let starts = group_starts(&slices, &state.selected, state.drag);
                let node_shift = state.drag.map_or(TimeMus::zero(), |drag| {
                    group_move_shift(
                        drag,
                        drag_delta,
                        grid,
                        bypass,
                        TimeMus::zero() - display_range.start,
                        &starts,
                    )
                });
                let bar_preview_shift = state.bar_drag.as_ref().map_or(TimeMus::zero(), |drag| {
                    let hover = ui
                        .input(|input| input.pointer.hover_pos())
                        .unwrap_or(drag.start_pointer);
                    let delta =
                        crate::timeline::interaction::pointer_delta(drag.start_pointer, hover, ppb);
                    let shift = crate::timeline::interaction::snapped_delta(
                        drag.origin.start,
                        delta,
                        grid,
                        bypass,
                    );
                    shift.max(clip_start - drag.origin.start)
                });

                if let Some(drag) = state.bar_drag.as_ref() {
                    let range = RangeMus::new(
                        drag.origin.start + bar_preview_shift,
                        drag.origin.end + bar_preview_shift,
                    );
                    state.bar_selection = range.ok();
                }

                if let Some(range) = state.bar_selection {
                    let x1 = timeline_grid.x_for_beat(range.start, content.left());
                    let x2 = timeline_grid.x_for_beat(range.end, content.left());
                    let rect = Rect::from_min_max(
                        egui::pos2(x1, lanes.top()),
                        egui::pos2(x2.max(x1 + 4.0), lanes.bottom()),
                    )
                    .intersect(lanes);
                    let response = ui.interact(
                        rect,
                        Id::new(("clip-editor-bar-drag", state.clip_id)),
                        Sense::drag(),
                    );
                    if response.drag_started() {
                        if let Some(pointer) = ui
                            .input(|input| input.pointer.press_origin())
                            .or_else(|| response.interact_pointer_pos())
                        {
                            state.bar_drag = Some(BarDrag {
                                start_pointer: pointer,
                                origin: range,
                                slices: {
                                    let local = RangeMus::new(
                                        range.start - clip_start,
                                        range.end - clip_start,
                                    )
                                    .unwrap_or(range);
                                    slices
                                        .iter()
                                        .filter(|slice| slice_intersects(&map, slice, local))
                                        .map(|slice| slice.id)
                                        .collect()
                                },
                            });
                        }
                    }
                    if response.drag_stopped() {
                        if let Some(drag) = state.bar_drag.take() {
                            let release = response
                                .interact_pointer_pos()
                                .or_else(|| ui.input(|input| input.pointer.hover_pos()))
                                .unwrap_or(drag.start_pointer);
                            let delta = crate::timeline::interaction::pointer_delta(
                                drag.start_pointer,
                                release,
                                ppb,
                            );
                            let delta = crate::timeline::interaction::snapped_delta(
                                drag.origin.start,
                                delta,
                                grid,
                                bypass,
                            );
                            let local = RangeMus::new(
                                drag.origin.start - clip_start,
                                drag.origin.end - clip_start,
                            )
                            .unwrap_or(drag.origin);
                            let selection = Selection::time(local, [ClipLane]);
                            let model = SliceModel {
                                slices: &slices,
                                map,
                                clip_length: clip.range.length(),
                                clip_start: clip.range.start,
                            };
                            if let Ok(plan) = edit::move_selection(&model, &selection, delta) {
                                apply_slice_edit(
                                    self, &mut state, &mut audio, clip.range, &map, &slices, plan,
                                );
                            }
                        }
                    }
                }

                if clip_start_x > lanes.left() {
                    clipped.rect_filled(
                        Rect::from_min_max(lanes.min, egui::pos2(clip_start_x, lanes.bottom())),
                        CornerRadius::ZERO,
                        Color32::from_black_alpha(150),
                    );
                }
                if clip_end_x < lanes.right() {
                    clipped.rect_filled(
                        Rect::from_min_max(egui::pos2(clip_end_x, lanes.top()), lanes.max),
                        CornerRadius::ZERO,
                        Color32::from_black_alpha(150),
                    );
                }
                timeline_grid.draw(&clipped, lanes);

                let slice_top = lanes.top() + 8.0;
                let slice_bottom = lanes.bottom() - 8.0;
                for slice in &slices {
                    let geometry = slice_geometry(
                        slice,
                        state.drag,
                        drag_delta,
                        node_shift,
                        &map,
                        grid,
                        bypass,
                        TimeMus::zero() - display_range.start,
                        state.selected.contains(&slice.id),
                    );
                    let start = if state
                        .bar_drag
                        .as_ref()
                        .is_some_and(|drag| drag.slices.contains(&slice.id))
                    {
                        slice.start + bar_preview_shift
                    } else {
                        geometry.start
                    };
                    let source_start = geometry.source_start;
                    let source_end = geometry.source_end;
                    let duration = geometry.duration(&map);
                    let x1 = timeline_grid.x_for_beat(clip_start + start, content.left());
                    let x2 =
                        timeline_grid.x_for_beat(clip_start + start + duration, content.left());
                    let full_rect = Rect::from_min_max(
                        egui::pos2(x1, slice_top),
                        egui::pos2(x2.max(x1 + 2.0), slice_bottom),
                    );
                    let slice_rect = full_rect.intersect(lanes);
                    let selected = state.selected.contains(&slice.id);
                    let color = crate::shell::clip_color(notwig_core::ClipKind::Audio);
                    let color = if clip.muted {
                        color.gamma_multiply(0.35)
                    } else {
                        color
                    };
                    painter.rect_filled(slice_rect, CornerRadius::same(3), color);
                    let asset_id = slice.asset_id.unwrap_or(audio.asset_id);
                    if let Some(peaks) = runtime.waveforms.get(&asset_id) {
                        let asset_samples = doc
                            .assets
                            .assets
                            .get(&asset_id)
                            .and_then(|asset| asset.duration_samples)
                            .unwrap_or(source_end);
                        crate::timeline::draw_waveform(
                            &painter,
                            full_rect.shrink2(Vec2::new(2.0, 4.0)),
                            lanes,
                            peaks,
                            asset_samples,
                            source_start,
                            source_end,
                        );
                    }
                    painter.rect_stroke(
                        slice_rect,
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
                    let name_height = 14.0_f32.min(slice_rect.height() * 0.42);
                    let title_rect = Rect::from_min_max(
                        slice_rect.min,
                        egui::pos2(slice_rect.right(), slice_rect.top() + name_height),
                    );
                    painter.rect_filled(
                        title_rect,
                        CornerRadius {
                            nw: 3,
                            ne: 3,
                            sw: 0,
                            se: 0,
                        },
                        Color32::from_black_alpha(52),
                    );
                    let label = doc.assets.assets.get(&asset_id).map_or("event", |asset| {
                        asset.uri.rsplit('/').next().unwrap_or("event")
                    });
                    painter.with_clip_rect(title_rect).text(
                        egui::pos2(title_rect.left() + 8.0, title_rect.center().y),
                        Align2::LEFT_CENTER,
                        label,
                        FontId::monospace(9.5),
                        Color32::from_black_alpha(235),
                    );
                    let slice_resp = ui.interact(
                        slice_rect,
                        Id::new(("slice", slice.id)),
                        Sense::click_and_drag(),
                    );
                    if slice_resp.clicked() {
                        let additive = ui.input(|i| i.modifiers.command || i.modifiers.shift);
                        state.select_node(slice.id, additive);
                    }
                    if slice_resp.drag_started() {
                        if let Some(pos) = ui
                            .input(|input| input.pointer.press_origin())
                            .or_else(|| slice_resp.interact_pointer_pos())
                        {
                            let shift = ui.input(|input| input.modifiers.shift);
                            if shift {
                                state.selected.clear();
                                state.bar_selection = None;
                                state.range_start = Some(
                                    timeline_grid
                                        .snap(timeline_grid.beat_for_x(pos.x, content.left()), true)
                                        .max(TimeMus::zero()),
                                );
                            } else {
                                if !state.selected.contains(&slice.id) {
                                    state.select_node(slice.id, false);
                                }
                                state.drag = Some(SliceDrag {
                                    slice_id: slice.id,
                                    mode: SliceDragMode::Move,
                                    origin_pointer: pos,
                                    origin_start: slice.start,
                                    origin_source_start: slice.source_start_sample,
                                    origin_source_end: slice.source_end_sample,
                                });
                            }
                        }
                    }
                    if slice_resp.dragged()
                        && let (Some(origin), Some(pos)) =
                            (state.range_start, slice_resp.interact_pointer_pos())
                    {
                        let beat = timeline_grid
                            .snap(timeline_grid.beat_for_x(pos.x, content.left()), true)
                            .max(TimeMus::zero());
                        if let Some(range) =
                            crate::timeline::interaction::selection_range(origin, beat)
                        {
                            state.select_bar(range);
                        }
                    }
                    if slice_resp.drag_stopped()
                        && state.range_start.take().is_none()
                        && let Some(drag) = state.drag.take()
                    {
                        finish_slice_drag(
                            self,
                            &mut state,
                            &mut audio,
                            slices.clone(),
                            drag,
                            slice_resp.interact_pointer_pos(),
                            &map,
                            bypass,
                            clip.range,
                            ppb,
                            grid,
                        );
                    }

                    for (mode, edge, label) in [
                        (SliceDragMode::TrimStart, slice_rect.left(), "start"),
                        (SliceDragMode::TrimEnd, slice_rect.right(), "end"),
                    ] {
                        let handle = Rect::from_center_size(
                            egui::pos2(edge, title_rect.center().y),
                            Vec2::new(7.0, title_rect.height().max(10.0)),
                        );
                        let trim = ui.interact(
                            handle,
                            Id::new(("slice-trim", slice.id, label)),
                            Sense::drag(),
                        );
                        if trim.hovered() || trim.dragged() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                            painter.rect_filled(handle, CornerRadius::same(2), Color32::WHITE);
                        }
                        if trim.drag_started()
                            && let Some(pos) = ui
                                .input(|input| input.pointer.press_origin())
                                .or_else(|| trim.interact_pointer_pos())
                        {
                            state.select_node(slice.id, false);
                            state.drag = Some(SliceDrag {
                                slice_id: slice.id,
                                mode,
                                origin_pointer: pos,
                                origin_start: slice.start,
                                origin_source_start: slice.source_start_sample,
                                origin_source_end: slice.source_end_sample,
                            });
                        }
                        if trim.drag_stopped()
                            && let Some(drag) = state.drag.take()
                        {
                            finish_slice_drag(
                                self,
                                &mut state,
                                &mut audio,
                                slices.clone(),
                                drag,
                                trim.interact_pointer_pos(),
                                &map,
                                bypass,
                                clip.range,
                                ppb,
                                grid,
                            );
                        }
                    }
                }

                if let Some(preview) = &self.drag_preview
                    && matches!(
                        preview.target,
                        crate::intent::DropAudioTarget::ClipEditor { clip_id }
                            if clip_id == state.clip_id
                    )
                {
                    let x1 = timeline_grid.x_for_beat(clip_start + preview.at, content.left());
                    let x2 = timeline_grid
                        .x_for_beat(clip_start + preview.at + preview.duration, content.left());
                    let rect = Rect::from_min_max(
                        egui::pos2(x1, slice_top),
                        egui::pos2(x2.max(x1 + 2.0), slice_bottom),
                    )
                    .intersect(lanes);
                    crate::timeline::draw_drop_preview(&painter, rect, &preview.path);
                }

                draw_clip_marker(
                    &clipped,
                    timeline_grid,
                    content,
                    ruler,
                    lanes,
                    display_range.start,
                    "CLIP START",
                    theme::ACCENT,
                    Align2::LEFT_TOP,
                );
                draw_clip_marker(
                    &clipped,
                    timeline_grid,
                    content,
                    ruler,
                    lanes,
                    display_range.end,
                    "CLIP END",
                    theme::TEXT_MUTED,
                    Align2::RIGHT_TOP,
                );
                for (edge, beat, label) in [
                    (ClipEdge::Start, display_range.start, "start"),
                    (ClipEdge::End, display_range.end, "end"),
                ] {
                    let x = timeline_grid.x_for_beat(beat, content.left());
                    let handle = Rect::from_center_size(
                        egui::pos2(x, ruler.center().y),
                        Vec2::new(14.0, RULER_H),
                    );
                    let response = ui.interact(
                        handle,
                        Id::new(("clip-edge", state.clip_id, label)),
                        Sense::drag(),
                    );
                    if response.hovered() || response.dragged() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                        painter.rect_filled(
                            Rect::from_min_max(
                                egui::pos2(x - 2.0, ruler.top()),
                                egui::pos2(x + 2.0, lanes.bottom()),
                            ),
                            CornerRadius::ZERO,
                            Color32::WHITE,
                        );
                    }
                    if response.drag_started() {
                        if let Some(pointer) = ui
                            .input(|input| input.pointer.press_origin())
                            .or_else(|| response.interact_pointer_pos())
                        {
                            state.edge_drag = Some(ClipEdgeDrag {
                                edge,
                                start_pointer: pointer,
                                origin: clip.range,
                            });
                        }
                    }
                    if response.dragged() {
                        if let Some(drag) = state.edge_drag {
                            let release = response
                                .interact_pointer_pos()
                                .or_else(|| ui.input(|input| input.pointer.hover_pos()))
                                .unwrap_or(drag.start_pointer);
                            state.preview_range =
                                edge_preview_range(drag, release, ppb, grid, bypass);
                        }
                    }
                    if response.drag_stopped() {
                        let drag = state.edge_drag.take();
                        let preview = state.preview_range.take();
                        if let (Some(drag), Some(range)) = (drag, preview)
                            && range != drag.origin
                        {
                            self.emit(UiIntent::SetClipRange {
                                clip_id: state.clip_id,
                                range,
                            });
                        }
                    }
                }

                if let Some(range) = state.bar_selection {
                    crate::timeline::draw_time_selection_fill(
                        &clipped,
                        timeline_grid,
                        content.left(),
                        lanes,
                        range,
                    );
                    let marker_rect = Rect::from_min_max(
                        egui::pos2(content.left(), ruler.top()),
                        egui::pos2(content.right(), lanes.bottom()),
                    );
                    crate::timeline::draw_time_selection_markers(
                        &clipped,
                        timeline_grid,
                        content.left(),
                        marker_rect,
                        ruler,
                        range,
                    );
                }
                crate::timeline::draw_playhead(
                    &clipped,
                    lanes,
                    content.left(),
                    timeline_grid,
                    runtime.playhead,
                );

                let source_rect = Rect::from_min_size(
                    egui::pos2(header.left() + 8.0, ruler.bottom() + 8.0),
                    Vec2::new(CLIP_HEADER_W - 16.0, 20.0),
                );
                if ui
                    .put(source_rect, egui::Button::new("+ Source").small())
                    .clicked()
                {
                    let mut slices = editor_slices(&clip, &audio);
                    let start = slices
                        .iter()
                        .map(|slice| slice.start + source_duration(&map, slice))
                        .max()
                        .unwrap_or(TimeMus::zero());
                    let source_end = doc
                        .assets
                        .assets
                        .get(&audio.asset_id)
                        .and_then(|asset| asset.duration_samples)
                        .unwrap_or(audio.source_end_sample);
                    slices.push(AudioSlice {
                        id: SliceId::from_uuid(Uuid::new_v4()),
                        start,
                        source_start_sample: audio.source_start_sample,
                        source_end_sample: source_end.max(audio.source_start_sample + 1),
                        gain_db: Db(0.0),
                        asset_id: None,
                    });
                    emit_slices(self, &mut state, &mut audio, slices);
                }

                let event_end = slices
                    .iter()
                    .map(|slice| clip.range.start + slice.start + source_duration(&map, slice))
                    .max()
                    .unwrap_or(clip.range.end);
                let content_end = event_end.max(clip.range.end) + TimeMus::new(4, 1);
                clip_editor_scrollbar(ui, &mut state, rect, content, content_end, ppb);
                true
            },
        );

        if open {
            self.clip_editor = Some(state);
        } else {
            self.close_clip_editor();
        }
    }
}

impl NotwigUi {
    fn clip_editor_placeholder(&mut self, ctx: &egui::Context) {
        let open = super::bottom_panel(
            ctx,
            super::PanelConfig {
                id: "clip_editor_panel",
                resizable: true,
                default_height: 260.0,
                min_height: 120.0,
                max_height: 700.0,
                handle_id: "clip-editor-panel",
                handle_tooltip: "Drag down to close",
            },
            |ui| {
                ui.label(
                    egui::RichText::new("Select an Arrangement Clip to edit.")
                        .color(theme::TEXT_MUTED),
                );
                true
            },
        );
        if !open {
            self.close_editor();
        }
    }
}

fn handle_clip_editor_actions(
    ui: &mut NotwigUi,
    ctx: &egui::Context,
    state: &mut ClipEditorState,
    audio: &mut AudioClipData,
    clip: &notwig_core::ClipEntity,
    map: &TempoMap,
    playhead: TimeMus,
) {
    if ui.edit_focus != crate::state::EditFocus::ClipEditor {
        return;
    }
    if ctx.wants_keyboard_input() {
        return;
    }
    let (copy, cut, paste, duplicate, delete) = ctx.input(|input| {
        let command = input.modifiers.command;
        let copy_event = input
            .events
            .iter()
            .any(|event| matches!(event, egui::Event::Copy));
        let cut_event = input
            .events
            .iter()
            .any(|event| matches!(event, egui::Event::Cut));
        let paste_event = input
            .events
            .iter()
            .any(|event| matches!(event, egui::Event::Paste(_)));
        (
            copy_event || (command && input.key_pressed(egui::Key::C)),
            cut_event || (command && input.key_pressed(egui::Key::X)),
            paste_event || (command && input.key_pressed(egui::Key::V)),
            command && input.key_pressed(egui::Key::D),
            input.key_pressed(egui::Key::Delete) || input.key_pressed(egui::Key::Backspace),
        )
    });
    let slices = editor_slices(clip, audio);
    let selected: Vec<SliceId> = state.selected.iter().copied().collect();
    let local_bar = state
        .bar_selection
        .and_then(|range| local_range(clip, range));
    let selection = if selected.is_empty() {
        local_bar.map_or_else(Selection::default, |range| {
            Selection::time(range, [ClipLane])
        })
    } else {
        Selection::objects(selected)
    };
    let model = SliceModel {
        slices: &slices,
        map: *map,
        clip_length: clip.range.length(),
        clip_start: clip.range.start,
    };

    if cut && !selection.is_empty() {
        if let Ok((extract, plan)) = edit::cut(&model, &selection) {
            apply_slice_edit(ui, state, audio, clip.range, map, &slices, plan);
            state.clipboard.extract = Some(extract);
        }
    } else {
        if copy
            && !selection.is_empty()
            && let Ok(extract) = edit::copy(&model, &selection)
        {
            state.clipboard.extract = Some(extract);
        }
        if delete
            && !selection.is_empty()
            && let Ok(plan) = edit::delete(&model, &selection)
        {
            apply_slice_edit(ui, state, audio, clip.range, map, &slices, plan);
        }
    }

    if paste && let Some(extract) = state.clipboard.extract.clone() {
        let at = (paste_target(state.bar_selection, playhead) - clip.range.start)
            .max(TimeMus::zero() - clip.range.start);
        if let Ok(plan) = edit::paste(&model, &extract, at) {
            apply_slice_edit(ui, state, audio, clip.range, map, &slices, plan);
        }
    }

    if duplicate && !selection.is_empty() {
        if let Ok(plan) = edit::duplicate(&model, &selection) {
            apply_slice_edit(ui, state, audio, clip.range, map, &slices, plan);
        }
    }
}

fn apply_slice_edit(
    ui: &mut NotwigUi,
    state: &mut ClipEditorState,
    audio: &mut AudioClipData,
    clip_range: RangeMus,
    map: &TempoMap,
    slices: &[AudioSlice],
    plan: Plan<ClipLane, SliceId, AudioSlice>,
) {
    let lowered = lower_slice_plan(slices, map, plan);
    match &lowered.selection {
        SliceSelection::Unchanged => {}
        SliceSelection::None => {
            state.selected.clear();
            state.bar_selection = None;
        }
        SliceSelection::Objects(objects) => {
            state.selected.clear();
            state.bar_selection = None;
            for (id, _) in objects {
                state.selected.insert(*id);
            }
        }
        SliceSelection::Time(local) => {
            state.selected.clear();
            state.bar_selection =
                RangeMus::new(local.start + clip_range.start, local.end + clip_range.start).ok();
        }
    }
    emit_slices(ui, state, audio, lowered.slices);
}

fn clip_editor_navigation(state: &mut ClipEditorState, ui: &egui::Ui, content: Rect) {
    if !ui.rect_contains_pointer(content) {
        return;
    }
    let Some(pointer) = ui.input(|input| input.pointer.hover_pos()) else {
        return;
    };
    let zoom_delta = ui.input(egui::InputState::zoom_delta);
    if (zoom_delta - 1.0).abs() > 1e-4 {
        state
            .viewport
            .zoom_at(pointer.x, content.left(), zoom_delta);
    }
    let middle = ui.input(|input| input.pointer.middle_down());
    if middle {
        let delta = ui.input(|input| input.pointer.delta());
        if delta.x.abs() > 0.0 {
            state
                .viewport
                .pan_beats(-f64::from(delta.x) / f64::from(state.viewport.pixels_per_beat));
        }
        if delta.y.abs() > 0.0 {
            state.viewport.zoom_at(
                pointer.x,
                content.left(),
                (-f64::from(delta.y) * 0.012).exp() as f32,
            );
        }
    }
    let scroll = ui.input(|input| input.smooth_scroll_delta);
    if !middle && scroll.x.abs() > scroll.y.abs() && scroll.x.abs() > 0.01 {
        state
            .viewport
            .pan_beats(-f64::from(scroll.x) / f64::from(state.viewport.pixels_per_beat));
    }
}

fn clip_editor_scrollbar(
    ui: &mut egui::Ui,
    state: &mut ClipEditorState,
    rect: Rect,
    content: Rect,
    content_end: TimeMus,
    ppb: f32,
) {
    let bar = Rect::from_min_size(
        egui::pos2(rect.left(), rect.bottom() + 2.0),
        Vec2::new(rect.width(), SCROLLBAR_H - 2.0),
    );
    let track = Rect::from_min_max(
        egui::pos2(content.left() + 4.0, bar.top() + 3.0),
        egui::pos2(bar.right() - 4.0, bar.bottom() - 3.0),
    );
    let visible = f64::from((content.width() / ppb.max(0.001)).max(1.0));
    if let Some(start) = crate::timeline::horizontal_scrollbar(
        ui,
        bar,
        track,
        content_end.as_f64(),
        visible,
        state.viewport.start,
        Id::new(("clip-editor-hscroll", state.clip_id)),
    ) {
        state.viewport.start = start;
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_clip_marker(
    painter: &egui::Painter,
    grid: TimelineGrid,
    content: Rect,
    ruler: Rect,
    lanes: Rect,
    beat: TimeMus,
    label: &str,
    color: Color32,
    align: Align2,
) {
    let x = grid.x_for_beat(beat, content.left());
    if x < content.left() - 1.0 || x > content.right() + 1.0 {
        return;
    }
    painter.line_segment(
        [egui::pos2(x, ruler.top()), egui::pos2(x, lanes.bottom())],
        Stroke::new(1.5_f32, color),
    );
    painter.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(x, ruler.top()),
            egui::pos2(x + 7.0, ruler.top() + 7.0),
            egui::pos2(x, ruler.top() + 7.0),
        ],
        color,
        Stroke::NONE,
    ));
    painter.text(
        egui::pos2(
            if align == Align2::RIGHT_TOP {
                x - 3.0
            } else {
                x + 3.0
            },
            ruler.top(),
        ),
        align,
        label,
        FontId::monospace(8.0),
        color,
    );
}

#[doc(hidden)]
#[derive(Debug, Clone, Copy)]
pub struct SliceGeometry {
    start: TimeMus,
    source_start: u64,
    source_end: u64,
}

impl SliceGeometry {
    fn duration(self, map: &TempoMap) -> TimeMus {
        map.samples_to_mus(self.source_end.saturating_sub(self.source_start))
    }
}

fn micro_time(value: f64) -> TimeMus {
    signed_time(value.max(0.0))
}

fn signed_time(value: f64) -> TimeMus {
    TimeMus::new((value * f64::from(MICRO)).round() as i64, MICRO)
}

fn clamp_slice_start(start: TimeMus, min_start: TimeMus) -> TimeMus {
    start.max(min_start)
}

fn local_range(clip: &notwig_core::ClipEntity, range: RangeMus) -> Option<RangeMus> {
    RangeMus::new(range.start - clip.range.start, range.end - clip.range.start).ok()
}

fn edge_preview_range(
    drag: ClipEdgeDrag,
    pointer: egui::Pos2,
    ppb: f32,
    grid: TimeMus,
    bypass: bool,
) -> Option<RangeMus> {
    let delta = f64::from(pointer.x - drag.start_pointer.x) / f64::from(ppb);
    let min_len = TimeMus::new(1, 16);
    match drag.edge {
        ClipEdge::Start => {
            let start = snap_time(micro_time(drag.origin.start.as_f64() + delta), grid, bypass)
                .max(TimeMus::zero())
                .min(drag.origin.end - min_len);
            RangeMus::new(start, drag.origin.end).ok()
        }
        ClipEdge::End => {
            let end = snap_time(micro_time(drag.origin.end.as_f64() + delta), grid, bypass)
                .max(drag.origin.start + min_len);
            RangeMus::new(drag.origin.start, end).ok()
        }
    }
}

fn group_starts(
    slices: &[AudioSlice],
    selected: &SelectionGroup<SliceId>,
    drag: Option<SliceDrag>,
) -> Vec<TimeMus> {
    let mut starts: Vec<TimeMus> = slices
        .iter()
        .filter(|slice| selected.contains(&slice.id))
        .map(|slice| slice.start)
        .collect();
    if starts.is_empty()
        && let Some(drag) = drag
    {
        starts.push(drag.origin_start);
    }
    starts
}

fn clamp_slice_group_shift(requested: TimeMus, starts: &[TimeMus], min_start: TimeMus) -> TimeMus {
    let Some(min) = starts.iter().copied().min() else {
        return requested;
    };
    requested.max(min_start - min)
}

fn group_move_shift(
    drag: SliceDrag,
    delta_mus: f64,
    grid: TimeMus,
    bypass: bool,
    min_start: TimeMus,
    starts: &[TimeMus],
) -> TimeMus {
    let anchor = clamp_slice_start(
        snap_time(
            micro_time(drag.origin_start.as_f64() + delta_mus),
            grid,
            bypass,
        ),
        min_start,
    );
    clamp_slice_group_shift(anchor - drag.origin_start, starts, min_start)
}

fn samples_for_delta(delta: TimeMus, map: &TempoMap) -> i64 {
    map.samples_delta(delta)
}

#[allow(clippy::too_many_arguments)]
fn slice_geometry(
    slice: &AudioSlice,
    drag: Option<SliceDrag>,
    delta_mus: f64,
    move_shift: TimeMus,
    map: &TempoMap,
    grid: TimeMus,
    bypass: bool,
    min_start: TimeMus,
    selected: bool,
) -> SliceGeometry {
    let mut geometry = SliceGeometry {
        start: slice.start,
        source_start: slice.source_start_sample,
        source_end: slice.source_end_sample,
    };
    if let Some(drag) = drag {
        if drag.slice_id == slice.id {
            match drag.mode {
                SliceDragMode::Move => {
                    geometry.start = clamp_slice_start(drag.origin_start + move_shift, min_start);
                }
                SliceDragMode::TrimStart => {
                    let start = clamp_slice_start(
                        snap_time(
                            micro_time(drag.origin_start.as_f64() + delta_mus),
                            grid,
                            bypass,
                        ),
                        min_start,
                    );
                    let source_delta = samples_for_delta(start - drag.origin_start, map);
                    let source = i128::from(drag.origin_source_start) + i128::from(source_delta);
                    geometry.source_start = u64::try_from(source)
                        .unwrap_or(0)
                        .min(drag.origin_source_end.saturating_sub(1));
                    geometry.start = start;
                }
                SliceDragMode::TrimEnd => {
                    let source_delta = samples_for_delta(signed_time(delta_mus), map);
                    let source = i128::from(drag.origin_source_end) + i128::from(source_delta);
                    let min_end = drag.origin_source_start.saturating_add(1);
                    geometry.source_end = u64::try_from(source).unwrap_or(min_end).max(min_end);
                }
            }
        } else if drag.mode == SliceDragMode::Move && selected {
            geometry.start = clamp_slice_start(slice.start + move_shift, min_start);
        }
    }
    geometry
}

#[allow(clippy::too_many_arguments)]
fn finish_slice_drag(
    ui: &mut NotwigUi,
    state: &mut ClipEditorState,
    audio: &mut AudioClipData,
    slices: Vec<AudioSlice>,
    drag: SliceDrag,
    end: Option<egui::Pos2>,
    map: &TempoMap,
    bypass: bool,
    clip_range: RangeMus,
    ppb: f32,
    grid: TimeMus,
) {
    let delta_mus = end.map_or(0.0, |pos| {
        f64::from(pos.x - drag.origin_pointer.x) / f64::from(ppb)
    });
    let starts = group_starts(&slices, &state.selected, Some(drag));
    let min_start = TimeMus::zero() - clip_range.start;
    let move_shift = group_move_shift(drag, delta_mus, grid, bypass, min_start, &starts);
    let model = SliceModel {
        slices: &slices,
        map: *map,
        clip_length: clip_range.length(),
        clip_start: clip_range.start,
    };
    let plan = match drag.mode {
        SliceDragMode::Move => {
            let objects: Vec<SliceId> = if state.selected.contains(&drag.slice_id) {
                state.selected.iter().copied().collect()
            } else {
                vec![drag.slice_id]
            };
            edit::move_selection(&model, &Selection::objects(objects), move_shift)
        }
        SliceDragMode::TrimStart => {
            let target = clamp_slice_start(
                snap_time(
                    micro_time(drag.origin_start.as_f64() + delta_mus),
                    grid,
                    bypass,
                ),
                min_start,
            );
            edit::trim_object(&model, drag.slice_id, edit::Edge::Start, target)
        }
        SliceDragMode::TrimEnd => {
            let duration = map.samples_to_mus(
                drag.origin_source_end
                    .saturating_sub(drag.origin_source_start),
            );
            let target = signed_time((drag.origin_start + duration).as_f64() + delta_mus);
            edit::trim_object(&model, drag.slice_id, edit::Edge::End, target)
        }
    };
    if let Ok(plan) = plan {
        apply_slice_edit(ui, state, audio, clip_range, map, &slices, plan);
    }
}

fn editor_slices(clip: &notwig_core::ClipEntity, audio: &AudioClipData) -> Vec<AudioSlice> {
    crate::timeline::audio_slices(clip, audio)
}

fn source_duration(map: &TempoMap, slice: &AudioSlice) -> TimeMus {
    map.samples_to_mus(
        slice
            .source_end_sample
            .saturating_sub(slice.source_start_sample),
    )
}

fn slice_interval(map: &TempoMap, slice: &AudioSlice) -> Option<RangeMus> {
    RangeMus::new(slice.start, slice.start + source_duration(map, slice)).ok()
}

fn slice_intersects(map: &TempoMap, slice: &AudioSlice, range: RangeMus) -> bool {
    slice_interval(map, slice).is_some_and(|interval| interval.intersects(range))
}

fn slice_segment(
    map: &TempoMap,
    slice: &AudioSlice,
    from: TimeMus,
    to: TimeMus,
) -> Option<AudioSlice> {
    let interval = slice_interval(map, slice)?;
    let start = from.max(interval.start);
    let end = to.min(interval.end);
    if end <= start {
        return None;
    }
    let start_delta = samples_for_delta(start - interval.start, map);
    let end_delta = samples_for_delta(end - interval.start, map);
    let source_start =
        u64::try_from(i128::from(slice.source_start_sample) + i128::from(start_delta))
            .unwrap_or(0)
            .min(slice.source_end_sample.saturating_sub(1));
    let source_end = u64::try_from(i128::from(slice.source_start_sample) + i128::from(end_delta))
        .unwrap_or(slice.source_end_sample)
        .min(slice.source_end_sample)
        .max(source_start + 1);
    if source_end <= source_start {
        return None;
    }
    let mut segment = slice.clone();
    segment.start = start;
    segment.source_start_sample = source_start;
    segment.source_end_sample = source_end;
    Some(segment)
}

fn delete_range(map: &TempoMap, slices: &[AudioSlice], range: RangeMus) -> Vec<AudioSlice> {
    if range.is_empty() {
        return slices.to_vec();
    }
    let mut out = Vec::new();
    for slice in slices {
        let Some(interval) = slice_interval(map, slice) else {
            continue;
        };
        if !interval.intersects(range) {
            out.push(slice.clone());
            continue;
        }
        let mut parts = Vec::new();
        if interval.start < range.start
            && let Some(left) = slice_segment(map, slice, interval.start, range.start)
        {
            parts.push(left);
        }
        if interval.end > range.end
            && let Some(right) = slice_segment(map, slice, range.end, interval.end)
        {
            parts.push(right);
        }
        for (index, mut part) in parts.into_iter().enumerate() {
            if index > 0 {
                part.id = SliceId::from_uuid(Uuid::new_v4());
            }
            out.push(part);
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClipLane;

pub struct SliceModel<'a> {
    slices: &'a [AudioSlice],
    map: TempoMap,
    clip_length: TimeMus,
    clip_start: TimeMus,
}

impl SliceModel<'_> {
    fn span(&self, slice: &AudioSlice) -> RangeMus {
        RangeMus::new(slice.start, slice.start + source_duration(&self.map, slice))
            .unwrap_or_else(|_| RangeMus::new(slice.start, slice.start).unwrap())
    }
}

impl EditModel for SliceModel<'_> {
    type Lane = ClipLane;
    type Object = SliceId;
    type Content = AudioSlice;

    fn editor(&self) -> EditorKind {
        EditorKind::AudioClip
    }

    fn container(&self) -> Option<RangeMus> {
        RangeMus::new(TimeMus::zero(), self.clip_length).ok()
    }

    fn lanes(&self) -> Vec<ClipLane> {
        vec![ClipLane]
    }

    fn lane_bounds(&self, _lane: ClipLane) -> LaneBounds {
        LaneBounds {
            start: TimeMus::zero() - self.clip_start,
            end: None,
        }
    }

    fn objects(&self, _lane: ClipLane) -> Vec<ObjectView<ClipLane, SliceId>> {
        let mut out: Vec<_> = self
            .slices
            .iter()
            .map(|slice| ObjectView {
                id: slice.id,
                lane: ClipLane,
                span: self.span(slice),
                kind: ObjectKind::Slice,
                extent: ObjectExtent::Span,
            })
            .collect();
        out.sort_by(|a, b| a.span.start.cmp(&b.span.start).then(a.id.cmp(&b.id)));
        out
    }

    fn object(&self, object: SliceId) -> Option<ObjectView<ClipLane, SliceId>> {
        self.slices
            .iter()
            .find(|slice| slice.id == object)
            .map(|slice| ObjectView {
                id: slice.id,
                lane: ClipLane,
                span: self.span(slice),
                kind: ObjectKind::Slice,
                extent: ObjectExtent::Span,
            })
    }

    fn trim_limits(&self, object: SliceId) -> TrimLimits {
        let Some(slice) = self.slices.iter().find(|slice| slice.id == object) else {
            return TrimLimits {
                min_start: TimeMus::zero(),
                max_start: TimeMus::zero(),
                min_end: TimeMus::zero(),
                max_end: Some(TimeMus::zero()),
            };
        };
        let span = self.span(slice);
        let left = self.map.samples_to_mus(slice.source_start_sample);
        TrimLimits {
            min_start: (span.start - left).max(TimeMus::zero() - self.clip_start),
            max_start: span.end - epsilon(),
            min_end: span.start + epsilon(),
            max_end: None,
        }
    }

    fn extract(&self, object: SliceId, window: RangeMus) -> Result<AudioSlice, EditError> {
        let slice = self
            .slices
            .iter()
            .find(|slice| slice.id == object)
            .ok_or_else(|| EditError::NotFound(object.to_string()))?;
        let span = self.span(slice);
        if window.is_empty() || window.start < span.start || window.end > span.end {
            return Err(EditError::InvalidExtraction(
                "window lies outside the slice".into(),
            ));
        }
        let mut segment = slice.clone();
        let start_delta = self.map.samples_delta(window.start - slice.start);
        let end_delta = self.map.samples_delta(window.end - slice.start);
        let max = i128::from(slice.source_end_sample);
        let source_start = (i128::from(slice.source_start_sample) + i128::from(start_delta))
            .clamp(0, max.saturating_sub(1));
        let source_end = (i128::from(slice.source_start_sample) + i128::from(end_delta))
            .clamp(source_start.saturating_add(1), max);
        segment.start = TimeMus::zero();
        segment.source_start_sample = u64::try_from(source_start).unwrap_or(0);
        segment.source_end_sample = u64::try_from(source_end).unwrap_or(0);
        Ok(segment)
    }

    fn accepts(&self, _lane: ClipLane, _kind: ObjectKind) -> bool {
        true
    }
}

pub enum SliceSelection {
    Unchanged,
    None,
    Objects(Vec<(SliceId, RangeMus)>),
    Time(RangeMus),
}

pub struct SliceLowered {
    pub slices: Vec<AudioSlice>,
    pub selection: SliceSelection,
}

pub fn lower_slice_plan(
    slices: &[AudioSlice],
    map: &TempoMap,
    plan: Plan<ClipLane, SliceId, AudioSlice>,
) -> SliceLowered {
    let mut out = slices.to_vec();
    let mut placed: Vec<Option<(SliceId, RangeMus)>> = vec![None; plan.ops.len()];
    for (index, op) in plan.ops.iter().enumerate() {
        match op {
            EditOp::SetContainer { .. } => {}
            EditOp::Erase { span, .. } => {
                out = delete_range(map, &out, *span);
            }
            EditOp::Remove { object } => {
                out.retain(|slice| slice.id != *object);
            }
            EditOp::Move { object, span, .. } => {
                if let Some(slice) = out.iter_mut().find(|slice| slice.id == *object) {
                    slice.start = span.start;
                }
            }
            EditOp::Trim { object, span } => {
                trim_slice(map, &mut out, *object, *span);
            }
            EditOp::Place { span, content, .. } => {
                let mut copy = content.clone();
                copy.id = SliceId::from_uuid(Uuid::new_v4());
                copy.start = span.start;
                placed[index] = Some((copy.id, *span));
                out.push(copy);
            }
        }
    }
    out.sort_by(|a, b| a.start.cmp(&b.start).then(a.id.cmp(&b.id)));
    let selection = match plan.next {
        NextSelection::Unchanged => SliceSelection::Unchanged,
        NextSelection::None => SliceSelection::None,
        NextSelection::Time(time) => SliceSelection::Time(time.span),
        NextSelection::Objects(refs) => {
            let mut objects = Vec::new();
            for reference in refs {
                match reference {
                    notwig_core::edit::ObjectRef::Existing(id) => {
                        if let Some(slice) = out.iter().find(|slice| slice.id == id) {
                            let span = RangeMus::new(
                                slice.start,
                                slice.start + source_duration(map, slice),
                            )
                            .unwrap_or_else(|_| RangeMus::new(slice.start, slice.start).unwrap());
                            objects.push((id, span));
                        }
                    }
                    notwig_core::edit::ObjectRef::Created(index) => {
                        if let Some((id, span)) = placed.get(index).and_then(|entry| *entry) {
                            objects.push((id, span));
                        }
                    }
                }
            }
            SliceSelection::Objects(objects)
        }
    };
    SliceLowered {
        slices: out,
        selection,
    }
}

fn trim_slice(map: &TempoMap, slices: &mut [AudioSlice], id: SliceId, span: RangeMus) {
    let Some(slice) = slices.iter_mut().find(|slice| slice.id == id) else {
        return;
    };
    let old_start = slice.start;
    let old_end = old_start + source_duration(map, slice);
    if span.start != old_start && span.end == old_end {
        let delta = map.samples_delta(span.start - old_start);
        let source = (i128::from(slice.source_start_sample) + i128::from(delta))
            .clamp(0, i128::from(slice.source_end_sample.saturating_sub(1)));
        slice.source_start_sample = u64::try_from(source).unwrap_or(0);
        slice.start = span.start;
    } else if span.end != old_end && span.start == old_start {
        let delta = map.samples_delta(span.end - old_end);
        let min = i128::from(slice.source_start_sample.saturating_add(1));
        let source = (i128::from(slice.source_end_sample) + i128::from(delta)).max(min);
        slice.source_end_sample =
            u64::try_from(source).unwrap_or(slice.source_start_sample.saturating_add(1));
    }
}

fn emit_slices(
    ui: &mut NotwigUi,
    state: &mut ClipEditorState,
    audio: &mut AudioClipData,
    slices: Vec<AudioSlice>,
) {
    audio.slices.clone_from(&slices);
    audio.slices_enabled = true;
    state
        .selected
        .retain(|id| slices.iter().any(|slice| slice.id == *id));
    ui.emit(UiIntent::SetClipSlices {
        clip_id: state.clip_id,
        slices,
    });
}

#[doc(hidden)]
pub mod test_api {
    #[allow(clippy::wildcard_imports)]
    use super::*;

    pub use super::{
        ClipEdge, ClipLane, SliceDrag, SliceDragMode, SliceGeometry, SliceLowered, SliceModel,
        SliceSelection, lower_slice_plan,
    };

    pub fn slice_drag(
        mode: SliceDragMode,
        slice_id: SliceId,
        origin_start: TimeMus,
        origin_source_start: u64,
        origin_source_end: u64,
    ) -> SliceDrag {
        SliceDrag {
            slice_id,
            mode,
            origin_pointer: egui::Pos2::ZERO,
            origin_start,
            origin_source_start,
            origin_source_end,
        }
    }

    pub fn set_drag_origin_start(drag: &mut SliceDrag, origin_start: TimeMus) {
        drag.origin_start = origin_start;
    }

    pub fn drag_origin_start(drag: &SliceDrag) -> TimeMus {
        drag.origin_start
    }

    pub fn clip_edge_drag(
        edge: ClipEdge,
        start_pointer: egui::Pos2,
        origin: RangeMus,
    ) -> ClipEdgeDrag {
        ClipEdgeDrag {
            edge,
            start_pointer,
            origin,
        }
    }

    pub fn group_move_shift(
        drag: SliceDrag,
        delta_mus: f64,
        grid: TimeMus,
        bypass: bool,
        min_start: TimeMus,
        starts: &[TimeMus],
    ) -> TimeMus {
        super::group_move_shift(drag, delta_mus, grid, bypass, min_start, starts)
    }

    pub fn clamp_slice_start(start: TimeMus, min_start: TimeMus) -> TimeMus {
        super::clamp_slice_start(start, min_start)
    }

    pub fn edge_preview_range(
        drag: ClipEdgeDrag,
        pointer: egui::Pos2,
        ppb: f32,
        grid: TimeMus,
        bypass: bool,
    ) -> Option<RangeMus> {
        super::edge_preview_range(drag, pointer, ppb, grid, bypass)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn slice_geometry(
        slice: &AudioSlice,
        drag: Option<SliceDrag>,
        delta_mus: f64,
        move_shift: TimeMus,
        map: &TempoMap,
        grid: TimeMus,
        bypass: bool,
        min_start: TimeMus,
        selected: bool,
    ) -> SliceGeometry {
        super::slice_geometry(
            slice, drag, delta_mus, move_shift, map, grid, bypass, min_start, selected,
        )
    }

    pub fn geometry_start(geometry: &SliceGeometry) -> TimeMus {
        geometry.start
    }

    pub fn geometry_source_start(geometry: &SliceGeometry) -> u64 {
        geometry.source_start
    }

    pub fn geometry_source_end(geometry: &SliceGeometry) -> u64 {
        geometry.source_end
    }

    pub fn geometry_duration(geometry: &SliceGeometry, map: &TempoMap) -> TimeMus {
        geometry.duration(map)
    }

    pub fn delete_range(map: &TempoMap, slices: &[AudioSlice], range: RangeMus) -> Vec<AudioSlice> {
        super::delete_range(map, slices, range)
    }

    pub fn source_duration(map: &TempoMap, slice: &AudioSlice) -> TimeMus {
        super::source_duration(map, slice)
    }

    pub fn slice_model(
        slices: &[AudioSlice],
        map: TempoMap,
        clip_length: TimeMus,
        clip_start: TimeMus,
    ) -> SliceModel<'_> {
        SliceModel {
            slices,
            map,
            clip_length,
            clip_start,
        }
    }

    pub fn emit_slices(
        ui: &mut NotwigUi,
        state: &mut ClipEditorState,
        audio: &mut AudioClipData,
        slices: Vec<AudioSlice>,
    ) {
        super::emit_slices(ui, state, audio, slices);
    }

    pub fn handle_clip_editor_actions(
        ui: &mut NotwigUi,
        ctx: &egui::Context,
        state: &mut ClipEditorState,
        audio: &mut AudioClipData,
        clip: &notwig_core::ClipEntity,
        map: &TempoMap,
        playhead: TimeMus,
    ) {
        super::handle_clip_editor_actions(ui, ctx, state, audio, clip, map, playhead);
    }
}
