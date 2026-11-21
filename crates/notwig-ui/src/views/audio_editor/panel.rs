use egui::{Align2, Color32, CornerRadius, FontId, Id, Rect, Sense, Stroke, StrokeKind, Vec2};
use notwig_core::edit::{self, Selection};
use notwig_core::{ClipPayload, ProjectDoc, RangeMus, TimeMus};

use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::shell::NotwigUi;
use crate::state::Viewport;
use crate::theme;
use crate::timeline::interaction::{drag_current_pos, drag_start_pos};
use crate::timeline::{ClipChrome, ClipChromeDrag, ClipChromeEdge, TimelineGrid};

use super::actions::{apply_slice_edit, handle_clip_editor_actions};
use super::geometry::{finish_slice_drag, group_move_shift, group_starts, slice_geometry};
use super::model::{ClipLane, SliceModel, editor_slices, slice_intersects, source_duration};
use super::navigation::{clip_editor_navigation, clip_editor_scrollbar};
use super::state::{BarDrag, ClipEdge, ClipEdgeDrag, ClipEditorState, SliceDrag, SliceDragMode};

const CLIP_HEADER_W: f32 = 120.0;
const RULER_H: f32 = 38.0;
const SCROLLBAR_H: f32 = 18.0;

pub(super) fn playhead_for_strip(runtime: &RuntimeView) -> TimeMus {
    crate::timeline::playhead_for_strip(runtime)
}

fn clear_empty_time_selection(state: &mut ClipEditorState) {
    if state
        .bar_selection
        .is_some_and(notwig_core::RangeMus::is_empty)
    {
        state.bar_selection = None;
    }
}

pub(super) fn event_color(doc: &ProjectDoc, clip: &notwig_core::ClipEntity) -> Color32 {
    crate::timeline::clip_color(doc, clip)
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
        let mut snap = self.state.snap;

        handle_clip_editor_actions(
            self,
            ctx,
            &mut state,
            &mut audio,
            &clip,
            &map,
            &runtime.clipboard,
            runtime.playhead,
        );

        let open = crate::views::panel::bottom_panel(
            ctx,
            crate::views::panel::PanelConfig {
                id: "clip_editor_panel",
                resizable: true,
                default_height: 420.0,
                min_height: 120.0,
                max_height: 700.0,
                collapse_height: 180.0,
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
                let grid = snap.step(ppb, beats_per_bar);
                let timeline_grid =
                    TimelineGrid::new(state.viewport.start, ppb, beats_per_bar, snap);
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
                    playhead: playhead_for_strip(runtime),
                    show_play_start: true,
                    loop_bottom_inset: 14.0,
                };
                if let Some(action) = strip.show(
                    ui,
                    strip_rect,
                    Id::new(("clip-editor-ruler", state.clip_id)),
                ) {
                    match action {
                        crate::timeline::BeatStripAction::Locate(beat) => {
                            self.emit(UiIntent::SetPlayStart(beat));
                        }
                        crate::timeline::BeatStripAction::SetLoop(range) => {
                            self.emit(UiIntent::SetLoopRange(range));
                        }
                    }
                }
                let clipped = painter.with_clip_rect(content);

                let bg_resp = ui.interact(
                    lanes,
                    Id::new(("clip-editor-bg", state.clip_id)),
                    Sense::click_and_drag(),
                );
                if bg_resp.clicked() {
                    if let Some(pos) = bg_resp.interact_pointer_pos() {
                        let bypass = snap.bypassed(ui.input(|input| input.modifiers));
                        let beat = timeline_grid
                            .snap(timeline_grid.beat_for_x(pos.x, content.left()), bypass)
                            .max(TimeMus::zero());
                        if let Ok(range) = RangeMus::new(beat, beat) {
                            state.select_bar(range);
                        }
                    }
                }
                if bg_resp.drag_started() {
                    if let Some(pos) = drag_start_pos(ui, &bg_resp) {
                        let bypass = snap.bypassed(ui.input(|input| input.modifiers));
                        let beat = timeline_grid
                            .snap(timeline_grid.beat_for_x(pos.x, content.left()), bypass)
                            .max(TimeMus::zero());
                        if let Ok(range) = RangeMus::new(beat, beat) {
                            state.select_bar(range);
                        }
                        state.range_start = Some(beat);
                    }
                }
                if bg_resp.dragged() {
                    if let (Some(origin), Some(pos)) =
                        (state.range_start, bg_resp.interact_pointer_pos())
                    {
                        let bypass = snap.bypassed(ui.input(|input| input.modifiers));
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
                    clear_empty_time_selection(&mut state);
                }

                let bypass = snap.bypassed(ui.input(|input| input.modifiers));
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
                    shift.max(TimeMus::zero() - drag.origin.start)
                });

                if let Some(drag) = state.bar_drag.as_ref() {
                    let range = RangeMus::new(
                        drag.origin.start + bar_preview_shift,
                        drag.origin.end + bar_preview_shift,
                    );
                    state.bar_selection = range.ok();
                }

                if let Some(range) = state.bar_selection.filter(|range| !range.is_empty()) {
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
                    if response.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                    }
                    if response.dragged() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                    }
                    if response.drag_started() {
                        if let Some(pointer) = drag_start_pos(ui, &response) {
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
                            let release = drag_current_pos(ui, &response, drag.start_pointer);
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
                    let color = event_color(doc, &clip);
                    let color = if clip.muted {
                        color.gamma_multiply(0.35)
                    } else {
                        color
                    };
                    painter.rect_filled(slice_rect, CornerRadius::same(3), color);
                    let asset_id = slice.asset_id.unwrap_or(audio.asset_id);
                    if let Some(waveform) = runtime.waveforms.get(&asset_id) {
                        let audio_start = start + map.samples_to_mus(geometry.silent_prefix);
                        let audio_duration =
                            map.samples_to_mus(source_end.saturating_sub(source_start));
                        let audio_x1 =
                            timeline_grid.x_for_beat(clip_start + audio_start, content.left());
                        let audio_x2 = timeline_grid
                            .x_for_beat(clip_start + audio_start + audio_duration, content.left());
                        let audio_rect = Rect::from_min_max(
                            egui::pos2(audio_x1, slice_top),
                            egui::pos2(audio_x2.max(audio_x1 + 1.0), slice_bottom),
                        );
                        crate::waveform::draw_waveform(
                            &painter,
                            audio_rect.shrink2(Vec2::new(2.0, 4.0)),
                            lanes,
                            waveform,
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
                        asset
                            .original_name
                            .as_deref()
                            .unwrap_or_else(|| asset.uri.rsplit('/').next().unwrap_or("event"))
                    });
                    painter.with_clip_rect(title_rect).text(
                        egui::pos2(title_rect.left() + 8.0, title_rect.center().y),
                        Align2::LEFT_CENTER,
                        label,
                        FontId::monospace(9.5),
                        Color32::from_black_alpha(235),
                    );
                    let body_rect = Rect::from_min_max(
                        egui::pos2(slice_rect.left(), title_rect.bottom()),
                        slice_rect.max,
                    );
                    let body_resp = ui.interact(
                        body_rect,
                        Id::new(("slice-body", slice.id)),
                        Sense::click_and_drag(),
                    );
                    if body_resp.clicked() {
                        let additive = ui.input(|i| i.modifiers.command || i.modifiers.shift);
                        state.select_node(slice.id, additive);
                    }
                    if body_resp.drag_started() {
                        if let Some(pos) = drag_start_pos(ui, &body_resp) {
                            let bypass = snap.bypassed(ui.input(|input| input.modifiers));
                            let beat = timeline_grid
                                .snap(timeline_grid.beat_for_x(pos.x, content.left()), bypass)
                                .max(TimeMus::zero());
                            if let Ok(range) = RangeMus::new(beat, beat) {
                                state.select_bar(range);
                            }
                            state.range_start = Some(beat);
                        }
                    }
                    if body_resp.dragged()
                        && let (Some(origin), Some(pos)) =
                            (state.range_start, body_resp.interact_pointer_pos())
                    {
                        let bypass = snap.bypassed(ui.input(|input| input.modifiers));
                        let beat = timeline_grid
                            .snap(timeline_grid.beat_for_x(pos.x, content.left()), bypass)
                            .max(TimeMus::zero());
                        if let Some(range) =
                            crate::timeline::interaction::selection_range(origin, beat)
                        {
                            state.select_bar(range);
                        }
                    }
                    if body_resp.drag_stopped() && state.range_start.take().is_some() {
                        clear_empty_time_selection(&mut state);
                    }

                    let title_resp = ui.interact(
                        title_rect,
                        Id::new(("slice-title", slice.id)),
                        Sense::click_and_drag(),
                    );
                    if title_resp.hovered() || title_resp.dragged() {
                        let cursor = if title_resp.dragged() {
                            egui::CursorIcon::Grabbing
                        } else {
                            egui::CursorIcon::Grab
                        };
                        ui.ctx().set_cursor_icon(cursor);
                    }
                    if title_resp.clicked() {
                        let additive = ui.input(|i| i.modifiers.command || i.modifiers.shift);
                        state.select_node(slice.id, additive);
                    }
                    if title_resp.drag_started() {
                        if let Some(pos) = drag_start_pos(ui, &title_resp) {
                            let shift = ui.input(|input| input.modifiers.shift);
                            if shift {
                                let beat = timeline_grid
                                    .snap(timeline_grid.beat_for_x(pos.x, content.left()), true)
                                    .max(TimeMus::zero());
                                if let Ok(range) = RangeMus::new(beat, beat) {
                                    state.select_bar(range);
                                }
                                state.range_start = Some(beat);
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
                                    origin_silent_prefix: slice.silent_prefix_samples,
                                });
                            }
                        }
                    }
                    if title_resp.dragged()
                        && let (Some(origin), Some(pos)) =
                            (state.range_start, title_resp.interact_pointer_pos())
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
                    if title_resp.drag_stopped() {
                        if state.range_start.take().is_some() {
                            clear_empty_time_selection(&mut state);
                        } else if let Some(drag) = state.drag.take() {
                            finish_slice_drag(
                                self,
                                &mut state,
                                &mut audio,
                                slices.clone(),
                                drag,
                                title_resp.interact_pointer_pos(),
                                &map,
                                bypass,
                                clip.range,
                                ppb,
                                grid,
                            );
                        }
                    }

                    for (mode, edge, label) in [
                        (SliceDragMode::TrimStart, slice_rect.left(), "start"),
                        (SliceDragMode::TrimEnd, slice_rect.right(), "end"),
                    ] {
                        let handle = Rect::from_min_max(
                            egui::pos2(edge - 3.5, title_rect.top()),
                            egui::pos2(edge + 3.5, title_rect.bottom()),
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
                            && let Some(pos) = drag_start_pos(ui, &trim)
                        {
                            state.select_node(slice.id, false);
                            state.drag = Some(SliceDrag {
                                slice_id: slice.id,
                                mode,
                                origin_pointer: pos,
                                origin_start: slice.start,
                                origin_source_start: slice.source_start_sample,
                                origin_source_end: slice.source_end_sample,
                                origin_silent_prefix: slice.silent_prefix_samples,
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
                    crate::timeline::draw_drop_preview(
                        &painter,
                        rect,
                        &preview.path,
                        preview.analyzing,
                    );
                }

                let mut chrome_drag = state.edge_drag.map(|drag| ClipChromeDrag {
                    edge: match drag.edge {
                        ClipEdge::Start => ClipChromeEdge::Start,
                        ClipEdge::End => ClipChromeEdge::End,
                    },
                    start_pointer: drag.start_pointer,
                    origin: drag.origin,
                });
                let was_dragging = chrome_drag.is_some();
                let chrome = crate::timeline::draw_clip_chrome(
                    ui,
                    &clipped,
                    ClipChrome {
                        grid: timeline_grid,
                        content,
                        ruler,
                        lanes,
                        range: display_range,
                        origin: clip.range,
                        id_salt: Id::new(("clip-editor-clip-edge", state.clip_id)),
                        ppb,
                        snap_step: grid,
                        bypass,
                        start_label: "",
                        end_label: "",
                        start_color: crate::timeline::clip_color(doc, &clip),
                        end_color: crate::timeline::clip_color(doc, &clip),
                    },
                    &mut chrome_drag,
                );
                state.edge_drag = chrome_drag.map(|drag| ClipEdgeDrag {
                    edge: match drag.edge {
                        ClipChromeEdge::Start => ClipEdge::Start,
                        ClipChromeEdge::End => ClipEdge::End,
                    },
                    start_pointer: drag.start_pointer,
                    origin: drag.origin,
                });
                if let Some(range) = chrome.preview {
                    state.preview_range = Some(range);
                }
                if was_dragging && state.edge_drag.is_none() {
                    state.preview_range = None;
                }
                if let Some(range) = chrome.committed {
                    self.emit(UiIntent::SetClipRange {
                        clip_id: state.clip_id,
                        range,
                    });
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
                if runtime.playing {
                    crate::timeline::draw_playhead(
                        &clipped,
                        lanes,
                        content.left(),
                        timeline_grid,
                        runtime.playhead,
                    );
                }

                let event_end = slices
                    .iter()
                    .map(|slice| clip.range.start + slice.start + source_duration(&map, slice))
                    .max()
                    .unwrap_or(clip.range.end);
                let content_end = event_end.max(clip.range.end) + TimeMus::new(4, 1);
                clip_editor_scrollbar(
                    ui,
                    &mut state,
                    &mut snap,
                    rect,
                    content,
                    content_end,
                    ppb,
                    beats_per_bar,
                );
                self.state.snap = snap;
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
        let open = crate::views::panel::bottom_panel(
            ctx,
            crate::views::panel::PanelConfig {
                id: "clip_editor_panel",
                resizable: true,
                default_height: 260.0,
                min_height: 80.0,
                max_height: 700.0,
                collapse_height: 120.0,
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
