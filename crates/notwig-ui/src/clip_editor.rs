use egui::{
    Align2, Color32, CornerRadius, FontId, Id, Rect, RichText, Sense, Stroke, StrokeKind, Vec2,
};
use notwig_core::sequencer::{AudioClipData, AudioSlice};
use notwig_core::{ClipId, ClipPayload, Db, ProjectDoc, RangeMus, SliceId, TempoMap, TimeMus};
use uuid::Uuid;

use crate::edit::{
    ClipboardMode, DuplicateSpan, EditTarget, bar_duplicate_target, clamp_bar_shift,
    clamp_node_shift, node_duplicate_target, paste_target,
};
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
    pub cursor: TimeMus,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SliceDragMode {
    Move,
    TrimStart,
    TrimEnd,
}

#[derive(Debug, Clone, Copy)]
struct SliceDrag {
    slice_id: SliceId,
    mode: SliceDragMode,
    origin_pointer: egui::Pos2,
    origin_start: TimeMus,
    origin_source_start: u64,
    origin_source_end: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClipEdge {
    Start,
    End,
}

#[derive(Debug, Clone, Copy)]
struct ClipEdgeDrag {
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

#[derive(Debug, Clone, PartialEq)]
struct SliceClipboard {
    entries: Vec<SliceClipboardEntry>,
    mode: ClipboardMode,
    length: TimeMus,
}

impl Default for SliceClipboard {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            mode: ClipboardMode::Nodes,
            length: TimeMus::zero(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct SliceClipboardEntry {
    offset: TimeMus,
    slice: AudioSlice,
}

#[derive(Debug, Clone, Default)]
struct PasteOutcome {
    slices: Vec<AudioSlice>,
    selected: Vec<SliceId>,
    bar: Option<RangeMus>,
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
            cursor: TimeMus::zero(),
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

    fn clear_selection(&mut self) {
        self.selected.clear();
        self.bar_selection = None;
    }

    fn select_node(&mut self, id: SliceId, additive: bool) {
        if additive {
            self.selected.toggle(id);
        } else {
            self.selected.clear();
            self.bar_selection = None;
            self.selected.select_only(id);
        }
    }

    fn select_bar(&mut self, range: RangeMus) {
        self.selected.clear();
        self.bar_selection = Some(range);
    }

    fn selection_mode(&self) -> Option<EditTarget> {
        EditTarget::active(self.selected.len(), self.bar_selection)
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
            state.viewport.start = clip.range.start;
            state.cursor = clip.range.start;
            state.initialized = true;
        }
        let clip_start = clip.range.start;
        let display_range = state.preview_range.unwrap_or(clip.range);
        let mut open = true;

        handle_clip_editor_actions(self, ctx, &mut state, &mut audio, &clip, &map);

        egui::TopBottomPanel::bottom("clip_editor_panel")
            .resizable(true)
            .default_height(420.0)
            .min_height(260.0)
            .max_height(700.0)
            .frame(
                egui::Frame::new()
                    .fill(Color32::BLACK)
                    .inner_margin(egui::Margin::symmetric(6, 4)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.small_button("×").clicked() {
                        open = false;
                    }
                    ui.label(
                        RichText::new(clip.name.as_deref().unwrap_or("Audio Clip"))
                            .strong()
                            .size(13.0),
                    );
                    ui.label(
                        RichText::new(format!(
                            "{} slices",
                            if audio.uses_slices() {
                                audio.slices.len()
                            } else {
                                1
                            }
                        ))
                        .small()
                        .color(theme::TEXT_MUTED),
                    );
                    ui.label(
                        RichText::new(format!(
                            "{} - {}",
                            format_beat(display_range.start),
                            format_beat(display_range.end)
                        ))
                        .monospace()
                        .small()
                        .color(theme::TEXT_MUTED),
                    );
                    if ui.small_button("+ Source").clicked() {
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
                        emit_slices(self, &mut state, &mut audio, &map, clip.range, slices);
                    }
                });
                ui.separator();

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
                painter.rect_filled(ruler, CornerRadius::ZERO, theme::PANEL_LIGHT);
                timeline_grid.draw(&painter, lanes);

                let clipped = painter.with_clip_rect(content);
                clipped.rect_filled(ruler, CornerRadius::ZERO, theme::PANEL_LIGHT);
                timeline_grid.draw_ruler(&clipped, ruler);

                let ruler_resp = ui.interact(
                    ruler,
                    Id::new(("clip-editor-ruler", state.clip_id)),
                    Sense::click_and_drag(),
                );
                if ruler_resp.clicked() || ruler_resp.dragged() {
                    if let Some(pos) = ruler_resp.interact_pointer_pos() {
                        let bypass = ui.input(|input| input.modifiers.shift);
                        state.cursor = clamp_clip_beat(
                            timeline_grid
                                .snap(timeline_grid.beat_for_x(pos.x, content.left()), bypass),
                            display_range,
                        );
                    }
                }

                let bg_resp = ui.interact(
                    content,
                    Id::new(("clip-editor-bg", state.clip_id)),
                    Sense::click_and_drag(),
                );
                if bg_resp.clicked() {
                    state.selected.clear();
                    state.bar_selection = None;
                    if let Some(pos) = bg_resp.interact_pointer_pos() {
                        let bypass = ui.input(|input| input.modifiers.shift);
                        state.cursor = clamp_clip_beat(
                            timeline_grid
                                .snap(timeline_grid.beat_for_x(pos.x, content.left()), bypass),
                            display_range,
                        );
                    }
                }
                if bg_resp.drag_started() {
                    state.selected.clear();
                    state.bar_selection = None;
                    if let Some(pos) = ui
                        .input(|input| input.pointer.press_origin())
                        .or_else(|| bg_resp.interact_pointer_pos())
                    {
                        let bypass = ui.input(|input| input.modifiers.shift);
                        let beat = clamp_clip_beat(
                            timeline_grid
                                .snap(timeline_grid.beat_for_x(pos.x, content.left()), bypass),
                            display_range,
                        );
                        state.range_start = Some(beat);
                    }
                }
                if bg_resp.dragged() {
                    if let (Some(origin), Some(pos)) =
                        (state.range_start, bg_resp.interact_pointer_pos())
                    {
                        let bypass = ui.input(|input| input.modifiers.shift);
                        let beat = clamp_clip_beat(
                            timeline_grid
                                .snap(timeline_grid.beat_for_x(pos.x, content.left()), bypass),
                            display_range,
                        );
                        if let Ok(range) = RangeMus::new(origin.min(beat), origin.max(beat))
                            && !range.is_empty()
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
                        display_range.length(),
                        &starts,
                    )
                });
                let bar_preview_shift = state.bar_drag.as_ref().map_or(TimeMus::zero(), |drag| {
                    let hover = ui
                        .input(|input| input.pointer.hover_pos())
                        .unwrap_or(drag.start_pointer);
                    let raw_shift = TimeMus::new(
                        (f64::from((hover.x - drag.start_pointer.x) / ppb) * f64::from(MICRO))
                            .round() as i64,
                        MICRO,
                    );
                    let requested = snap_time(drag.origin.start + raw_shift, grid, bypass);
                    let origin =
                        RangeMus::new(drag.origin.start - clip_start, drag.origin.end - clip_start)
                            .unwrap_or(drag.origin);
                    bar_shift(
                        &slices,
                        origin,
                        requested - drag.origin.start,
                        display_range.length(),
                        &map,
                    )
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
                            let raw_shift = TimeMus::new(
                                (f64::from((release.x - drag.start_pointer.x) / ppb)
                                    * f64::from(MICRO))
                                .round() as i64,
                                MICRO,
                            );
                            let requested = snap_time(drag.origin.start + raw_shift, grid, bypass);
                            let origin = RangeMus::new(
                                drag.origin.start - clip_start,
                                drag.origin.end - clip_start,
                            )
                            .unwrap_or(drag.origin);
                            let shift = bar_shift(
                                &slices,
                                origin,
                                requested - drag.origin.start,
                                display_range.length(),
                                &map,
                            );
                            let updated = apply_bar_move(&map, slices.clone(), origin, shift);
                            state.bar_selection =
                                RangeMus::new(drag.origin.start + shift, drag.origin.end + shift)
                                    .ok();
                            emit_slices(self, &mut state, &mut audio, &map, clip.range, updated);
                        }
                    }
                }

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
                        display_range.length(),
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
                    let slice_rect = Rect::from_min_max(
                        egui::pos2(x1, slice_top),
                        egui::pos2(x2.max(x1 + 2.0), slice_bottom),
                    )
                    .intersect(lanes);
                    let selected = state.selected.contains(&slice.id);
                    painter.rect_filled(
                        slice_rect,
                        CornerRadius::same(3),
                        if selected {
                            theme::CLIP_AUDIO.gamma_multiply(1.1)
                        } else {
                            theme::CLIP_AUDIO
                        },
                    );
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
                            slice_rect.shrink2(Vec2::new(2.0, 4.0)),
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
                            if selected { 2.0_f32 } else { 1.0_f32 },
                            if selected {
                                Color32::WHITE
                            } else {
                                Color32::from_black_alpha(140)
                            },
                        ),
                        StrokeKind::Inside,
                    );
                    painter.text(
                        egui::pos2(slice_rect.left() + 4.0, slice_rect.top() + 3.0),
                        Align2::LEFT_TOP,
                        doc.assets
                            .assets
                            .get(&audio.asset_id)
                            .map_or("slice", |asset| {
                                asset.uri.rsplit('/').next().unwrap_or("slice")
                            }),
                        FontId::monospace(8.0),
                        Color32::WHITE,
                    );
                    let title_rect = Rect::from_min_max(
                        slice_rect.min,
                        egui::pos2(
                            slice_rect.right(),
                            (slice_rect.top() + 16.0).min(slice_rect.bottom()),
                        ),
                    );
                    let resp = ui.interact(
                        title_rect,
                        Id::new(("slice", slice.id)),
                        Sense::click_and_drag(),
                    );
                    if resp.clicked() {
                        let additive = ui.input(|i| i.modifiers.command || i.modifiers.shift);
                        state.select_node(slice.id, additive);
                    }
                    if resp.drag_started() {
                        if let Some(pos) = ui
                            .input(|input| input.pointer.press_origin())
                            .or_else(|| resp.interact_pointer_pos())
                        {
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
                    if resp.drag_stopped()
                        && let Some(drag) = state.drag.take()
                    {
                        finish_slice_drag(
                            self,
                            &mut state,
                            &mut audio,
                            slices.clone(),
                            drag,
                            resp.interact_pointer_pos(),
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
                    painter.text(
                        egui::pos2(rect.left() + 4.0, rect.top() + 3.0),
                        Align2::LEFT_TOP,
                        preview
                            .path
                            .file_name()
                            .map_or("audio", |name| name.to_str().unwrap_or("audio")),
                        FontId::monospace(8.0),
                        Color32::WHITE,
                    );
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
                            self.emit(UiIntent::EditClipRange {
                                clip_id: state.clip_id,
                                range,
                            });
                        }
                    }
                }

                if let Some(range) = state.bar_selection {
                    let x1 = timeline_grid
                        .x_for_beat(range.start, content.left())
                        .max(lanes.left());
                    let x2 = timeline_grid
                        .x_for_beat(range.end, content.left())
                        .min(lanes.right());
                    if x2 > x1 {
                        clipped.rect_filled(
                            Rect::from_min_max(
                                egui::pos2(x1, ruler.bottom()),
                                egui::pos2(x2, lanes.bottom()),
                            ),
                            CornerRadius::ZERO,
                            Color32::from_rgba_unmultiplied(255, 210, 120, 34),
                        );
                    }
                }
                draw_bar_selection_markers(
                    &clipped,
                    timeline_grid,
                    content,
                    ruler,
                    lanes,
                    state.bar_selection,
                );
                draw_cursor(&clipped, timeline_grid, content, ruler, lanes, state.cursor);

                painter.text(
                    egui::pos2(header.left() + 8.0, header.top() + 10.0),
                    Align2::LEFT_TOP,
                    "AUDIO SLICES",
                    FontId::monospace(10.0),
                    theme::TEXT_MUTED,
                );
                painter.text(
                    egui::pos2(header.left() + 8.0, header.top() + 28.0),
                    Align2::LEFT_TOP,
                    format!("{} bars", format_beat(display_range.length())),
                    FontId::monospace(9.0),
                    theme::GRID_STRONG,
                );

                clip_editor_scrollbar(ui, &mut state, rect, content, display_range.end, ppb);
            });

        if open {
            self.clip_editor = Some(state);
        } else {
            self.close_clip_editor();
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
) {
    if ui.edit_focus != crate::state::EditFocus::ClipEditor {
        return;
    }
    if ctx.wants_keyboard_input() {
        return;
    }
    let (copy, cut, paste, duplicate, delete) = ctx.input(|input| {
        (
            input.modifiers.command && input.key_pressed(egui::Key::C),
            input.modifiers.command && input.key_pressed(egui::Key::X),
            input.modifiers.command && input.key_pressed(egui::Key::V),
            input.modifiers.command && input.key_pressed(egui::Key::D),
            input.key_pressed(egui::Key::Delete) || input.key_pressed(egui::Key::Backspace),
        )
    });
    let slices = editor_slices(clip, audio);
    let local_bar = state
        .bar_selection
        .and_then(|range| local_range(clip, range));
    if copy || cut {
        if !state.selected.is_empty() {
            state.clipboard = copy_nodes(map, &slices, &state.selected);
        } else if let Some(range) = local_bar {
            state.clipboard = copy_range(map, &slices, range);
        }
    }
    if cut || delete {
        match state.selection_mode() {
            Some(EditTarget::Nodes) => {
                let selected = state.selected.iter().copied().collect::<Vec<_>>();
                let updated = delete_nodes(&slices, &selected);
                state.clear_selection();
                emit_slices(ui, state, audio, map, clip.range, updated);
            }
            Some(EditTarget::Bar) => {
                if let Some(range) = local_bar {
                    let updated = delete_range(map, &slices, range);
                    state.bar_selection = None;
                    emit_slices(ui, state, audio, map, clip.range, updated);
                }
            }
            None => {}
        }
    }
    if paste && !state.clipboard.entries.is_empty() {
        let local_target = (paste_target(state.bar_selection, state.cursor) - clip.range.start)
            .max(TimeMus::zero());
        let outcome = paste_clipboard(&slices, &state.clipboard, local_target);
        state.selected.clear();
        for id in &outcome.selected {
            state.selected.insert(*id);
        }
        state.bar_selection = outcome
            .bar
            .and_then(|range| offset_range(range, clip.range.start));
        emit_slices(ui, state, audio, map, clip.range, outcome.slices);
    }
    if duplicate {
        match state.selection_mode() {
            Some(EditTarget::Nodes) => {
                let selected = state.selected.iter().copied().collect::<Vec<_>>();
                let clipboard = copy_nodes(map, &slices, &state.selected);
                let spans: Vec<DuplicateSpan<SliceId>> = selected
                    .iter()
                    .filter_map(|id| slices.iter().find(|slice| slice.id == *id))
                    .map(|slice| DuplicateSpan {
                        id: slice.id,
                        range: RangeMus::new(
                            slice.start,
                            slice.start + source_duration(map, slice),
                        )
                        .unwrap_or(RangeMus::new(slice.start, slice.start).unwrap()),
                    })
                    .collect();
                let target = node_duplicate_target(&spans)
                    .unwrap_or((state.cursor - clip.range.start).max(TimeMus::zero()));
                let outcome = paste_clipboard(&slices, &clipboard, target);
                state.selected.clear();
                for id in &outcome.selected {
                    state.selected.insert(*id);
                }
                state.bar_selection = None;
                emit_slices(ui, state, audio, map, clip.range, outcome.slices);
            }
            Some(EditTarget::Bar) => {
                if let Some(range) = state.bar_selection {
                    let clipboard = copy_range(map, &slices, range);
                    let outcome = paste_clipboard(&slices, &clipboard, bar_duplicate_target(range));
                    state.selected.clear();
                    state.bar_selection = outcome.bar;
                    emit_slices(ui, state, audio, map, clip.range, outcome.slices);
                }
            }
            None => {}
        }
    }
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
    clip_length: TimeMus,
    ppb: f32,
) {
    let bar = Rect::from_min_size(
        egui::pos2(rect.left(), rect.bottom() + 2.0),
        Vec2::new(rect.width(), SCROLLBAR_H - 2.0),
    );
    let painter = ui.painter();
    painter.rect_filled(bar, CornerRadius::ZERO, theme::PANEL_LIGHT);
    let track = Rect::from_min_max(
        egui::pos2(content.left() + 4.0, bar.top() + 3.0),
        egui::pos2(bar.right() - 4.0, bar.bottom() - 3.0),
    );
    let visible = f64::from((content.width() / ppb.max(0.001)).max(1.0));
    let content_end = (clip_length.as_f64() + 4.0).max(visible);
    let start = state
        .viewport
        .start
        .as_f64()
        .clamp(0.0, (content_end - visible).max(0.0));
    let thumb_x = track.left() + (start / content_end) as f32 * track.width();
    let thumb_w = ((visible / content_end) as f32 * track.width())
        .max(24.0)
        .min(track.width());
    let thumb = Rect::from_min_size(
        egui::pos2(thumb_x.min(track.right() - thumb_w), track.top()),
        Vec2::new(thumb_w, track.height()),
    );
    painter.rect_filled(track, CornerRadius::same(3), theme::GRID);
    painter.rect_filled(thumb, CornerRadius::same(3), theme::TEXT_MUTED);
    let response = ui.interact(
        bar,
        Id::new(("clip-editor-hscroll", state.clip_id)),
        Sense::click_and_drag(),
    );
    if (response.clicked() || response.dragged())
        && let Some(pos) = response.interact_pointer_pos()
    {
        let fraction = f64::from(((pos.x - track.left()) / track.width()).clamp(0.0, 1.0));
        let max_start = (content_end - visible).max(0.0);
        state.viewport.start = TimeMus::new(
            ((fraction * max_start) * f64::from(MICRO)).round() as i64,
            MICRO,
        );
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

fn draw_bar_selection_markers(
    painter: &egui::Painter,
    grid: TimelineGrid,
    content: Rect,
    ruler: Rect,
    lanes: Rect,
    range: Option<RangeMus>,
) {
    let Some(range) = range else {
        return;
    };
    for (beat, label) in [(range.start, "IN"), (range.end, "OUT")] {
        let x = grid.x_for_beat(beat, content.left());
        if x < content.left() - 1.0 || x > content.right() + 1.0 {
            continue;
        }
        painter.line_segment(
            [egui::pos2(x, ruler.top()), egui::pos2(x, lanes.bottom())],
            Stroke::new(1.5_f32, Color32::WHITE),
        );
        painter.text(
            egui::pos2(x + 3.0, ruler.bottom() - 11.0),
            Align2::LEFT_TOP,
            label,
            FontId::monospace(8.0),
            Color32::WHITE,
        );
    }
}

fn draw_cursor(
    painter: &egui::Painter,
    grid: TimelineGrid,
    content: Rect,
    ruler: Rect,
    lanes: Rect,
    cursor: TimeMus,
) {
    let x = grid.x_for_beat(cursor, content.left());
    if x < content.left() - 1.0 || x > content.right() + 1.0 {
        return;
    }
    painter.line_segment(
        [egui::pos2(x, ruler.top()), egui::pos2(x, lanes.bottom())],
        Stroke::new(1.0_f32, theme::ACCENT),
    );
    painter.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(x - 4.0, ruler.top()),
            egui::pos2(x + 4.0, ruler.top()),
            egui::pos2(x, ruler.top() + 6.0),
        ],
        theme::ACCENT,
        Stroke::NONE,
    ));
}

#[derive(Debug, Clone, Copy)]
struct SliceGeometry {
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

fn max_slice_start(clip_length: TimeMus) -> TimeMus {
    clip_length
        .max(TimeMus::zero())
        .min((clip_length - TimeMus::new(1, MICRO)).max(TimeMus::zero()))
}

fn clamp_slice_start(start: TimeMus, clip_length: TimeMus) -> TimeMus {
    start.max(TimeMus::zero()).min(max_slice_start(clip_length))
}

fn clamp_clip_beat(beat: TimeMus, range: RangeMus) -> TimeMus {
    beat.max(range.start).min(range.end)
}

fn local_range(clip: &notwig_core::ClipEntity, range: RangeMus) -> Option<RangeMus> {
    let start = range.start.max(clip.range.start).min(clip.range.end);
    let end = range.end.min(clip.range.end);
    RangeMus::new(start - clip.range.start, end - clip.range.start).ok()
}

fn offset_range(range: RangeMus, offset: TimeMus) -> Option<RangeMus> {
    RangeMus::new(range.start + offset, range.end + offset).ok()
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

fn format_beat(beat: TimeMus) -> String {
    format!("{:.3}", beat.as_f64())
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

fn clamp_slice_group_shift(
    requested: TimeMus,
    starts: &[TimeMus],
    clip_length: TimeMus,
) -> TimeMus {
    clamp_node_shift(requested, starts, Some(max_slice_start(clip_length)))
}

fn group_move_shift(
    drag: SliceDrag,
    delta_mus: f64,
    grid: TimeMus,
    bypass: bool,
    clip_length: TimeMus,
    starts: &[TimeMus],
) -> TimeMus {
    let anchor = clamp_slice_start(
        snap_time(
            micro_time(drag.origin_start.as_f64() + delta_mus),
            grid,
            bypass,
        ),
        clip_length,
    );
    clamp_slice_group_shift(anchor - drag.origin_start, starts, clip_length)
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
    clip_length: TimeMus,
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
                    geometry.start = clamp_slice_start(drag.origin_start + move_shift, clip_length);
                }
                SliceDragMode::TrimStart => {
                    let start = clamp_slice_start(
                        snap_time(
                            micro_time(drag.origin_start.as_f64() + delta_mus),
                            grid,
                            bypass,
                        ),
                        clip_length,
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
            geometry.start = clamp_slice_start(slice.start + move_shift, clip_length);
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
    let move_shift = group_move_shift(drag, delta_mus, grid, bypass, clip_range.length(), &starts);
    let mut updated = slices;
    for entry in &mut updated {
        let selected = state.selected.contains(&entry.id);
        let geometry = slice_geometry(
            entry,
            Some(drag),
            delta_mus,
            move_shift,
            map,
            grid,
            bypass,
            clip_range.length(),
            selected,
        );
        if entry.id == drag.slice_id || (drag.mode == SliceDragMode::Move && selected) {
            entry.start = geometry.start;
            entry.source_start_sample = geometry.source_start;
            entry.source_end_sample = geometry.source_end;
        }
    }
    emit_slices(ui, state, audio, map, clip_range, updated);
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

fn copy_nodes(
    map: &TempoMap,
    slices: &[AudioSlice],
    selected: &SelectionGroup<SliceId>,
) -> SliceClipboard {
    let mut selected_slices: Vec<AudioSlice> = slices
        .iter()
        .filter(|slice| selected.contains(&slice.id))
        .cloned()
        .collect();
    if selected_slices.is_empty() {
        return SliceClipboard::default();
    }
    selected_slices.sort_by_key(|slice| slice.start);
    let base = selected_slices[0].start;
    let entries: Vec<SliceClipboardEntry> = selected_slices
        .into_iter()
        .map(|slice| SliceClipboardEntry {
            offset: slice.start - base,
            slice,
        })
        .collect();
    SliceClipboard {
        length: entries
            .iter()
            .map(|entry| entry.offset + source_duration(map, &entry.slice))
            .max()
            .unwrap_or(TimeMus::zero()),
        entries,
        mode: ClipboardMode::Nodes,
    }
}

fn copy_range(map: &TempoMap, slices: &[AudioSlice], range: RangeMus) -> SliceClipboard {
    if range.is_empty() {
        return SliceClipboard::default();
    }
    let entries = slices
        .iter()
        .filter_map(|slice| {
            let interval = slice_interval(map, slice)?;
            let start = range.start.max(interval.start);
            let end = range.end.min(interval.end);
            let mut segment = slice_segment(map, slice, start, end)?;
            segment.start = start - range.start;
            Some(SliceClipboardEntry {
                offset: segment.start,
                slice: segment,
            })
        })
        .collect();
    SliceClipboard {
        entries,
        mode: ClipboardMode::Time,
        length: range.length(),
    }
}

fn paste_clipboard(
    slices: &[AudioSlice],
    clipboard: &SliceClipboard,
    target: TimeMus,
) -> PasteOutcome {
    let mut out = slices.to_vec();
    let mut selected = Vec::new();
    let target = target.max(TimeMus::zero());
    for entry in &clipboard.entries {
        let mut copy = entry.slice.clone();
        copy.id = SliceId::from_uuid(Uuid::new_v4());
        copy.start = target + entry.offset;
        selected.push(copy.id);
        out.push(copy);
    }
    let bar = match clipboard.mode {
        ClipboardMode::Nodes => None,
        ClipboardMode::Time => RangeMus::new(target, target + clipboard.length).ok(),
    };
    PasteOutcome {
        slices: out,
        selected,
        bar,
    }
}

fn delete_nodes(slices: &[AudioSlice], selected: &[SliceId]) -> Vec<AudioSlice> {
    slices
        .iter()
        .filter(|slice| !selected.contains(&slice.id))
        .cloned()
        .collect()
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

fn bar_shift(
    slices: &[AudioSlice],
    origin: RangeMus,
    requested: TimeMus,
    clip_length: TimeMus,
    map: &TempoMap,
) -> TimeMus {
    let starts: Vec<TimeMus> = slices
        .iter()
        .filter(|slice| slice_intersects(map, slice, origin))
        .map(|slice| slice.start)
        .collect();
    clamp_bar_shift(
        origin,
        requested,
        &starts,
        Some(clip_length),
        Some(max_slice_start(clip_length)),
    )
}

fn apply_bar_move(
    map: &TempoMap,
    slices: Vec<AudioSlice>,
    origin: RangeMus,
    shift: TimeMus,
) -> Vec<AudioSlice> {
    slices
        .into_iter()
        .map(|mut slice| {
            if slice_intersects(map, &slice, origin) {
                slice.start = slice.start + shift;
            }
            slice
        })
        .collect()
}

fn emit_slices(
    ui: &mut NotwigUi,
    state: &mut ClipEditorState,
    audio: &mut AudioClipData,
    map: &TempoMap,
    clip_range: RangeMus,
    slices: Vec<AudioSlice>,
) {
    let required = slices
        .iter()
        .map(|slice| slice.start + source_duration(map, slice))
        .max()
        .unwrap_or(TimeMus::zero());
    if required > clip_range.length()
        && let Ok(range) = RangeMus::new(clip_range.start, clip_range.start + required)
    {
        ui.emit(UiIntent::EditClipRange {
            clip_id: state.clip_id,
            range,
        });
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use notwig_core::SampleRate;

    fn map() -> TempoMap {
        TempoMap {
            tempo_bpm: 120.0,
            sample_rate: SampleRate::DEFAULT,
        }
    }

    fn slice_at(id: u128, start: i64, source_start: u64, source_end: u64) -> AudioSlice {
        AudioSlice {
            id: SliceId::from_uuid(Uuid::from_u128(id)),
            start: TimeMus::new(start, 1),
            source_start_sample: source_start,
            source_end_sample: source_end,
            gain_db: Db(0.0),
            asset_id: None,
        }
    }

    fn slice() -> AudioSlice {
        slice_at(7, 1, 0, 48_000)
    }

    fn audio() -> AudioClipData {
        AudioClipData {
            asset_id: notwig_core::AssetId::from_uuid(Uuid::from_u128(9)),
            source_start_sample: 0,
            source_end_sample: 48_000,
            gain_db: Db(0.0),
            fade_in_samples: 0,
            fade_out_samples: 0,
            fade_shape: notwig_core::sequencer::FadeShape::Linear,
            slices: vec![slice()],
            slices_enabled: true,
        }
    }

    fn drag(mode: SliceDragMode) -> SliceDrag {
        SliceDrag {
            slice_id: slice().id,
            mode,
            origin_pointer: egui::pos2(0.0, 0.0),
            origin_start: TimeMus::new(1, 1),
            origin_source_start: 0,
            origin_source_end: 48_000,
        }
    }

    fn selection(ids: &[u128]) -> SelectionGroup<SliceId> {
        let mut selected = SelectionGroup::default();
        for id in ids {
            selected.insert(SliceId::from_uuid(Uuid::from_u128(*id)));
        }
        selected
    }

    #[test]
    fn node_and_bar_selection_are_mutually_exclusive() {
        let mut state = ClipEditorState::new(ClipId::nil());
        state.select_node(slice().id, false);
        assert!(!state.selected.is_empty());
        assert!(state.bar_selection.is_none());

        state.select_bar(RangeMus::new(TimeMus::new(1, 1), TimeMus::new(2, 1)).unwrap());
        assert!(state.selected.is_empty());
        assert!(state.bar_selection.is_some());

        state.select_node(slice().id, false);
        assert!(!state.selected.is_empty());
        assert!(state.bar_selection.is_none());
    }

    #[test]
    fn additive_node_selection_preserves_bar_selection_and_uses_node_mode() {
        let mut state = ClipEditorState::new(ClipId::nil());
        let range = RangeMus::new(TimeMus::new(1, 1), TimeMus::new(3, 1)).unwrap();
        state.select_bar(range);
        state.select_node(slice().id, true);
        assert_eq!(state.bar_selection, Some(range));
        assert!(state.selected.contains(&slice().id));
        assert_eq!(state.selection_mode(), Some(EditTarget::Nodes));
    }

    #[test]
    fn move_snaps_anchor_and_preserves_group_offsets() {
        let drag = drag(SliceDragMode::Move);
        let starts = [TimeMus::new(0, 1), TimeMus::new(1, 1)];
        let shift = group_move_shift(
            drag,
            1.1,
            TimeMus::new(1, 4),
            false,
            TimeMus::new(4, 1),
            &starts,
        );
        assert_eq!(shift, TimeMus::new(1, 1));
    }

    #[test]
    fn move_clamps_group_inside_clip() {
        let mut drag = drag(SliceDragMode::Move);
        drag.origin_start = TimeMus::new(3, 1);
        let starts = [TimeMus::new(3, 1), TimeMus::new(2, 1)];
        let shift = group_move_shift(
            drag,
            10.0,
            TimeMus::new(1, 4),
            false,
            TimeMus::new(4, 1),
            &starts,
        );
        let anchor = clamp_slice_start(drag.origin_start + shift, TimeMus::new(4, 1));
        assert_eq!(anchor, TimeMus::new(3_999_999, 1_000_000));
    }

    #[test]
    fn alt_bypasses_move_snap() {
        let drag = drag(SliceDragMode::Move);
        let starts = [TimeMus::new(1, 1)];
        let shift = group_move_shift(
            drag,
            1.1 / 3.0,
            TimeMus::new(1, 4),
            true,
            TimeMus::new(4, 1),
            &starts,
        );
        assert!((shift.as_f64() - 1.1 / 3.0).abs() < 1e-6);
    }

    #[test]
    fn trim_start_keeps_right_edge_fixed() {
        let geometry = slice_geometry(
            &slice(),
            Some(drag(SliceDragMode::TrimStart)),
            1.0,
            TimeMus::zero(),
            &map(),
            TimeMus::new(1, 4),
            false,
            TimeMus::new(4, 1),
            true,
        );
        assert_eq!(geometry.start, TimeMus::new(2, 1));
        assert_eq!(geometry.source_start, 24_000);
        assert_eq!(
            geometry.start + geometry.duration(&map()),
            TimeMus::new(3, 1)
        );
    }

    #[test]
    fn trim_end_extends_source_and_clamps_non_empty() {
        let geometry = slice_geometry(
            &slice(),
            Some(drag(SliceDragMode::TrimEnd)),
            1.0,
            TimeMus::zero(),
            &map(),
            TimeMus::new(1, 4),
            false,
            TimeMus::new(4, 1),
            true,
        );
        assert_eq!(geometry.source_end, 72_000);
        assert_eq!(geometry.duration(&map()), TimeMus::new(3, 1));

        let collapsed = slice_geometry(
            &slice(),
            Some(drag(SliceDragMode::TrimEnd)),
            -100.0,
            TimeMus::zero(),
            &map(),
            TimeMus::new(1, 4),
            false,
            TimeMus::new(4, 1),
            true,
        );
        assert_eq!(collapsed.source_end, 1);
    }

    #[test]
    fn node_copy_paste_preserves_offsets_and_assigns_new_ids() {
        let slices = vec![slice_at(1, 1, 0, 24_000), slice_at(2, 3, 24_000, 48_000)];
        let selected = selection(&[1, 2]);
        let clipboard = copy_nodes(&map(), &slices, &selected);
        assert_eq!(clipboard.mode, ClipboardMode::Nodes);
        assert_eq!(clipboard.entries[0].offset, TimeMus::zero());
        assert_eq!(clipboard.entries[1].offset, TimeMus::new(2, 1));
        let outcome = paste_clipboard(&slices, &clipboard, TimeMus::new(5, 1));
        assert_eq!(outcome.slices.len(), 4);
        assert_eq!(outcome.slices[2].start, TimeMus::new(5, 1));
        assert_eq!(outcome.slices[3].start, TimeMus::new(7, 1));
        assert_ne!(outcome.slices[2].id, slices[0].id);
        assert_ne!(outcome.slices[3].id, slices[1].id);
        assert_eq!(outcome.selected.len(), 2);
        assert!(outcome.bar.is_none());
    }

    #[test]
    fn bar_copy_trims_source_and_offsets_to_range() {
        let slices = vec![slice_at(1, 1, 0, 96_000)];
        let range = RangeMus::new(TimeMus::new(3, 2), TimeMus::new(7, 2)).unwrap();
        let clipboard = copy_range(&map(), &slices, range);
        assert_eq!(clipboard.mode, ClipboardMode::Time);
        assert_eq!(clipboard.length, TimeMus::new(2, 1));
        assert_eq!(clipboard.entries.len(), 1);
        assert_eq!(clipboard.entries[0].offset, TimeMus::zero());
        assert_eq!(clipboard.entries[0].slice.source_start_sample, 12_000);
        assert_eq!(clipboard.entries[0].slice.source_end_sample, 60_000);
    }

    #[test]
    fn delete_range_splits_a_slice_in_the_middle() {
        let slices = vec![slice_at(1, 1, 0, 96_000)];
        let range = RangeMus::new(TimeMus::new(2, 1), TimeMus::new(3, 1)).unwrap();
        let updated = delete_range(&map(), &slices, range);
        assert_eq!(updated.len(), 2);
        assert_eq!(updated[0].start, TimeMus::new(1, 1));
        assert_eq!(updated[0].source_end_sample, 24_000);
        assert_eq!(updated[1].start, TimeMus::new(3, 1));
        assert_eq!(updated[1].source_start_sample, 48_000);
        assert_ne!(updated[0].id, updated[1].id);
    }

    #[test]
    fn delete_range_trims_edges_and_removes_covered_slices() {
        let slices = vec![slice_at(1, 1, 0, 96_000)];
        let left = delete_range(
            &map(),
            &slices,
            RangeMus::new(TimeMus::new(0, 1), TimeMus::new(2, 1)).unwrap(),
        );
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].start, TimeMus::new(2, 1));
        assert_eq!(left[0].source_start_sample, 24_000);

        let right = delete_range(
            &map(),
            &slices,
            RangeMus::new(TimeMus::new(4, 1), TimeMus::new(8, 1)).unwrap(),
        );
        assert_eq!(right.len(), 1);
        assert_eq!(right[0].start, TimeMus::new(1, 1));
        assert_eq!(right[0].source_end_sample, 72_000);

        let covered = delete_range(
            &map(),
            &slices,
            RangeMus::new(TimeMus::zero(), TimeMus::new(8, 1)).unwrap(),
        );
        assert_eq!(covered.len(), 0);
    }

    #[test]
    fn cut_range_copies_before_deleting() {
        let slices = vec![slice_at(1, 1, 0, 96_000)];
        let range = RangeMus::new(TimeMus::new(2, 1), TimeMus::new(3, 1)).unwrap();
        let clipboard = copy_range(&map(), &slices, range);
        let remaining = delete_range(&map(), &slices, range);
        assert_eq!(clipboard.entries.len(), 1);
        assert_eq!(clipboard.entries[0].slice.source_start_sample, 24_000);
        assert_eq!(remaining.len(), 2);
    }

    #[test]
    fn bar_paste_places_trimmed_content_at_the_cursor_and_selects_the_range() {
        let slices = vec![slice_at(1, 0, 0, 96_000)];
        let range = RangeMus::new(TimeMus::new(1, 1), TimeMus::new(3, 1)).unwrap();
        let clipboard = copy_range(&map(), &slices, range);
        let outcome = paste_clipboard(&slices, &clipboard, TimeMus::new(5, 1));
        assert_eq!(outcome.selected.len(), 1);
        assert_eq!(
            outcome.bar,
            RangeMus::new(TimeMus::new(5, 1), TimeMus::new(7, 1)).ok()
        );
        assert_eq!(outcome.slices[1].start, TimeMus::new(5, 1));
        assert_eq!(outcome.slices[1].source_start_sample, 24_000);
    }

    #[test]
    fn node_cut_copies_before_deleting() {
        let slices = vec![slice_at(1, 0, 0, 24_000), slice_at(2, 1, 0, 24_000)];
        let selected = selection(&[1]);
        let clipboard = copy_nodes(&map(), &slices, &selected);
        let remaining = delete_nodes(&slices, &[SliceId::from_uuid(Uuid::from_u128(1))]);
        assert_eq!(clipboard.entries.len(), 1);
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, slices[1].id);
    }

    #[test]
    fn delete_nodes_removes_only_selected_nodes() {
        let slices = vec![slice_at(1, 0, 0, 24_000), slice_at(2, 1, 0, 24_000)];
        let updated = delete_nodes(&slices, &[SliceId::from_uuid(Uuid::from_u128(1))]);
        assert_eq!(updated.len(), 1);
        assert_eq!(updated[0].id, slices[1].id);
    }

    #[test]
    fn duplicate_nodes_places_clone_after_the_selection() {
        let slices = vec![slice_at(1, 1, 0, 24_000), slice_at(2, 3, 0, 24_000)];
        let selected = selection(&[1, 2]);
        let clipboard = copy_nodes(&map(), &slices, &selected);
        let target = TimeMus::new(4, 1);
        let outcome = paste_clipboard(&slices, &clipboard, target);
        assert_eq!(outcome.slices[2].start, TimeMus::new(4, 1));
        assert_eq!(outcome.slices[3].start, TimeMus::new(6, 1));
        assert_eq!(outcome.selected.len(), 2);
    }

    #[test]
    fn duplicate_range_places_clone_at_range_end_and_selects_it() {
        let slices = vec![slice_at(1, 0, 0, 96_000)];
        let range = RangeMus::new(TimeMus::new(1, 1), TimeMus::new(3, 1)).unwrap();
        let clipboard = copy_range(&map(), &slices, range);
        let outcome = paste_clipboard(&slices, &clipboard, range.end);
        assert_eq!(outcome.selected.len(), 1);
        assert_eq!(
            outcome.bar,
            RangeMus::new(TimeMus::new(3, 1), TimeMus::new(5, 1)).ok()
        );
        assert_eq!(outcome.slices[1].start, TimeMus::new(3, 1));
        assert_eq!(outcome.slices[1].source_start_sample, 24_000);
    }

    #[test]
    fn bar_move_shifts_intersecting_slices_and_clamps() {
        let slices = vec![slice_at(1, 1, 0, 24_000), slice_at(2, 4, 0, 24_000)];
        let origin = RangeMus::new(TimeMus::new(1, 1), TimeMus::new(2, 1)).unwrap();
        let shift = bar_shift(
            &slices,
            origin,
            TimeMus::new(1, 1),
            TimeMus::new(8, 1),
            &map(),
        );
        let moved = apply_bar_move(&map(), slices.clone(), origin, shift);
        assert_eq!(move_to(&moved, 1), TimeMus::new(2, 1));
        assert_eq!(move_to(&moved, 2), TimeMus::new(4, 1));

        let left = bar_shift(
            &slices,
            origin,
            TimeMus::new(-8, 1),
            TimeMus::new(8, 1),
            &map(),
        );
        assert_eq!(left, TimeMus::new(-1, 1));
    }

    fn move_to(slices: &[AudioSlice], id: u128) -> TimeMus {
        slices
            .iter()
            .find(|slice| slice.id == SliceId::from_uuid(Uuid::from_u128(id)))
            .unwrap()
            .start
    }

    #[test]
    fn clip_edge_drag_resizes_and_clamps() {
        let origin = RangeMus::new(TimeMus::new(2, 1), TimeMus::new(6, 1)).unwrap();
        let start = edge_preview_range(
            ClipEdgeDrag {
                edge: ClipEdge::Start,
                start_pointer: egui::pos2(0.0, 0.0),
                origin,
            },
            egui::pos2(80.0, 0.0),
            80.0,
            TimeMus::new(1, 4),
            false,
        )
        .unwrap();
        assert_eq!(
            start,
            RangeMus::new(TimeMus::new(3, 1), TimeMus::new(6, 1)).unwrap()
        );

        let end = edge_preview_range(
            ClipEdgeDrag {
                edge: ClipEdge::End,
                start_pointer: egui::pos2(0.0, 0.0),
                origin,
            },
            egui::pos2(80.0, 0.0),
            80.0,
            TimeMus::new(1, 4),
            false,
        )
        .unwrap();
        assert_eq!(
            end,
            RangeMus::new(TimeMus::new(2, 1), TimeMus::new(7, 1)).unwrap()
        );

        let clamped = edge_preview_range(
            ClipEdgeDrag {
                edge: ClipEdge::Start,
                start_pointer: egui::pos2(0.0, 0.0),
                origin,
            },
            egui::pos2(10_000.0, 0.0),
            80.0,
            TimeMus::new(1, 4),
            false,
        )
        .unwrap();
        assert!(clamped.length() > TimeMus::zero());
        assert_eq!(clamped.end, TimeMus::new(6, 1));
    }

    #[test]
    fn zoom_keeps_local_beat_under_pointer() {
        let mut state = ClipEditorState::new(ClipId::nil());
        let left = 100.0;
        let pointer = 420.0;
        let before = state.viewport.start.as_f64()
            + f64::from((pointer - left) / state.viewport.pixels_per_beat);
        state.viewport.zoom_at(pointer, left, 2.0);
        let after = state.viewport.start.as_f64()
            + f64::from((pointer - left) / state.viewport.pixels_per_beat);
        assert!((before - after).abs() < 1e-6);
    }

    #[test]
    fn command_x_cuts_the_clip_editor_bar_selection() {
        let mut ui = NotwigUi::default();
        ui.edit_focus = crate::state::EditFocus::ClipEditor;
        let map = map();
        let mut audio = audio();
        let clip = notwig_core::ClipEntity {
            id: ClipId::nil(),
            track_id: notwig_core::TrackId::nil(),
            kind: notwig_core::sequencer::ClipKind::Audio,
            range: RangeMus::new(TimeMus::zero(), TimeMus::new(4, 1)).unwrap(),
            name: None,
            color: None,
            muted: false,
            payload: ClipPayload::Audio(audio.clone()),
        };
        let mut state = ClipEditorState::new(clip.id);
        state.select_bar(RangeMus::new(TimeMus::new(1, 1), TimeMus::new(2, 1)).unwrap());
        let ctx = egui::Context::default();
        let _ = ctx.run(
            egui::RawInput {
                modifiers: egui::Modifiers {
                    command: true,
                    ..Default::default()
                },
                events: vec![egui::Event::Key {
                    key: egui::Key::X,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers {
                        command: true,
                        ..Default::default()
                    },
                }],
                ..Default::default()
            },
            |ctx| {
                handle_clip_editor_actions(&mut ui, ctx, &mut state, &mut audio, &clip, &map);
            },
        );
        let slices = ui
            .intents
            .iter()
            .find_map(|intent| match intent {
                UiIntent::SetClipSlices { slices, .. } => Some(slices.clone()),
                _ => None,
            })
            .expect("cut should emit slices");
        assert_eq!(slices.len(), 1);
        assert_eq!(slices[0].start, TimeMus::new(2, 1));
        assert_eq!(slices[0].source_start_sample, 24_000);
    }

    #[test]
    fn deleting_last_slice_keeps_clip_in_slice_mode() {
        let mut ui = NotwigUi::default();
        let mut state = ClipEditorState::new(ClipId::nil());
        let mut audio = audio();
        emit_slices(
            &mut ui,
            &mut state,
            &mut audio,
            &map(),
            RangeMus::new(TimeMus::zero(), TimeMus::new(4, 1)).unwrap(),
            Vec::new(),
        );
        assert_eq!(audio.slices.len(), 0);
        assert!(audio.slices_enabled);
        assert!(ui.intents.iter().any(|intent| matches!(
            intent,
            UiIntent::SetClipSlices { slices, .. } if slices.is_empty()
        )));
    }

    #[test]
    fn emit_grows_clip_for_out_of_range_slices() {
        let mut ui = NotwigUi::default();
        let mut state = ClipEditorState::new(ClipId::nil());
        let mut audio = audio();
        let mut out = slice();
        out.start = TimeMus::new(5, 1);
        emit_slices(
            &mut ui,
            &mut state,
            &mut audio,
            &map(),
            RangeMus::new(TimeMus::zero(), TimeMus::new(4, 1)).unwrap(),
            vec![out],
        );
        assert!(ui.intents.iter().any(|intent| matches!(
            intent,
            UiIntent::EditClipRange { range, .. } if *range
                == RangeMus::new(TimeMus::zero(), TimeMus::new(7, 1)).unwrap()
        )));
        assert!(
            ui.intents
                .iter()
                .position(|intent| matches!(intent, UiIntent::EditClipRange { .. }))
                < ui.intents
                    .iter()
                    .position(|intent| matches!(intent, UiIntent::SetClipSlices { .. }))
        );
    }
}
