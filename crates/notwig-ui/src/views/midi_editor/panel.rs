use std::collections::{BTreeMap, BTreeSet};

use egui::{Align2, Color32, CornerRadius, FontId, Id, Rect, Sense, Stroke, StrokeKind, Vec2};
use notwig_core::edit;
use notwig_core::{
    ClipEntity, ClipPayload, MidiClipData, Norm01, NoteEntity, NoteId, ProjectDoc, RangeMus,
    TimeMus, TrackId,
};
use uuid::Uuid;

use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::shell::NotwigUi;
use crate::state::Viewport;
use crate::theme;
use crate::timeline::{BeatStrip, ClipChrome, TimelineGrid, playhead_for_strip};

use super::actions::handle_midi_editor_actions;
use super::geometry::{
    ROW_HEIGHT, beat_from_content_time, clamp_pitch_scroll, clamp_pitch_shift, clamp_time_shift,
    content_time_from_beat, pitch_for_y, snapped_shift, velocity_for_y, velocity_y, y_for_pitch,
};
use super::model::{
    MidiModel, Pitch, apply_note_lowered, create_note_clearing, default_note_end, lower_note_plan,
};
use super::navigation::{SCROLLBAR_H, midi_editor_navigation, midi_editor_scrollbar};
use super::state::{
    MarqueeDrag, MidiEditorState, NoteDrag, NoteDragMode, NoteOrigin, VelocityDrag,
};

const KEY_WIDTH: f32 = 56.0;
pub(super) const RULER_H: f32 = 38.0;
const VELOCITY_H: f32 = 62.0;
const MIN_NOTE_LEN: i64 = 1;
const MIN_NOTE_DEN: u32 = 16;

#[derive(Debug, Clone)]
struct NoteVisual {
    note: NoteEntity,
    span: RangeMus,
    rect: Rect,
}

impl NotwigUi {
    pub(crate) fn midi_editor_panel(
        &mut self,
        ctx: &egui::Context,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
    ) {
        let Some(mut state) = self.midi_editor.clone() else {
            self.midi_editor_placeholder(ctx);
            return;
        };
        let Some(clip) = doc.arranger.clips.get(&state.clip_id).cloned() else {
            self.close_midi_editor();
            return;
        };
        let ClipPayload::Midi(original) = &clip.payload else {
            self.close_midi_editor();
            return;
        };
        let mut midi = original.clone();
        if !state.initialized {
            state.viewport.start = (clip.range.start - TimeMus::new(4, 1)).max(TimeMus::zero());
            let average = if midi.notes.is_empty() {
                60
            } else {
                let total: u32 = midi.notes.iter().map(|note| u32::from(note.pitch)).sum();
                u8::try_from(total / midi.notes.len() as u32).unwrap_or(60)
            };
            state.pitch_scroll = (127.0 - f32::from(average) - 8.0).clamp(0.0, 96.0);
            state.initialized = true;
        }
        state.sync_notes(&midi.notes);
        let display_range = state.preview_range.unwrap_or(clip.range);
        let ppb = state
            .viewport
            .pixels_per_beat
            .clamp(Viewport::MIN_PPB, Viewport::MAX_PPB);
        let beats_per_bar = doc.meta.time_sig_num.max(1);
        let mut snap = self.state.snap;
        let grid = snap.step(ppb, beats_per_bar);

        handle_midi_editor_actions(
            self,
            ctx,
            &mut state,
            &mut midi,
            &clip,
            grid,
            runtime.playhead,
            doc.meta.tempo_bpm,
        );

        let open = crate::views::panel::bottom_panel(
            ctx,
            crate::views::panel::PanelConfig {
                id: "midi_editor_panel",
                resizable: true,
                default_height: 440.0,
                min_height: 160.0,
                max_height: 800.0,
                collapse_height: 220.0,
                handle_id: "midi-editor-panel",
                handle_tooltip: "Drag down to close",
            },
            |ui| {
                let width = ui.available_width().max(320.0);
                let height = ui.available_height().max(200.0);
                let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
                let body = Rect::from_min_max(
                    rect.min,
                    egui::pos2(rect.right(), rect.bottom() - SCROLLBAR_H),
                );
                let header = Rect::from_min_max(
                    body.min,
                    egui::pos2(rect.left() + KEY_WIDTH, body.bottom()),
                );
                let content = Rect::from_min_max(egui::pos2(header.right(), body.top()), body.max);
                let ruler = Rect::from_min_size(content.min, Vec2::new(content.width(), RULER_H));
                let velocity = Rect::from_min_max(
                    egui::pos2(content.left(), content.bottom() - VELOCITY_H),
                    content.max,
                );
                let note_area = Rect::from_min_max(
                    egui::pos2(content.left(), ruler.bottom()),
                    egui::pos2(content.right(), velocity.min.y),
                );
                self.midi_editor_content_rect = content;
                if ui.rect_contains_pointer(rect) && ui.input(|input| input.pointer.any_pressed()) {
                    self.edit_focus = crate::state::EditFocus::MidiEditor;
                }
                state.pitch_scroll = clamp_pitch_scroll(state.pitch_scroll, note_area.height());
                midi_editor_navigation(&mut state, ui, content, note_area);

                // Navigation can change the zoom; recompute the scale before
                // laying out this frame so the grid and note geometry agree.
                let ppb = state
                    .viewport
                    .pixels_per_beat
                    .clamp(Viewport::MIN_PPB, Viewport::MAX_PPB);
                let grid = snap.step(ppb, beats_per_bar);

                let painter = ui.painter().clone();
                painter.rect_filled(rect, CornerRadius::ZERO, Color32::BLACK);
                painter.rect_filled(header, CornerRadius::ZERO, theme::PANEL_LIGHT);
                painter.rect_filled(note_area, CornerRadius::ZERO, theme::LANE);
                painter.rect_filled(velocity, CornerRadius::ZERO, theme::PANEL_LIGHT);

                let timeline_grid =
                    TimelineGrid::new(state.viewport.start, ppb, beats_per_bar, snap);
                let strip_rect =
                    Rect::from_min_max(rect.min, egui::pos2(content.right(), rect.top() + RULER_H));
                let strip = BeatStrip {
                    grid: timeline_grid,
                    header_width: KEY_WIDTH,
                    header_label: Some("MIDI EDITOR"),
                    loop_range: if doc.transport.loop_enabled {
                        doc.transport.loop_range
                    } else {
                        None
                    },
                    time_selection: state
                        .time_selection()
                        .map(|range| absolute_time_range(range, &clip, &midi)),
                    play_start: runtime.play_start,
                    playhead: playhead_for_strip(runtime),
                    show_play_start: true,
                    loop_bottom_inset: 14.0,
                };
                if let Some(action) = strip.show(
                    ui,
                    strip_rect,
                    Id::new(("midi-editor-ruler", state.clip_id)),
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

                let clipped = painter.with_clip_rect(note_area);
                let content_clipped = painter.with_clip_rect(content);
                let now = ui.input(|input| input.time);
                state
                    .preview_highlights
                    .retain(|_, expiry| !expiry.is_finite() || *expiry > now);
                let playing_pitches = played_pitches(&midi, &clip, runtime);
                let pressed_key = draw_pitch_rows(
                    ui,
                    &clipped,
                    &painter,
                    header,
                    note_area,
                    state.pitch_scroll,
                    &state.preview_highlights,
                    &playing_pitches,
                    now,
                );
                if let Some(pitch) = pressed_key
                    && state.held_key.is_none_or(|(_, _, held)| held != pitch)
                {
                    let note_id = NoteId::from_uuid(Uuid::new_v4());
                    self.emit(UiIntent::PreviewNote {
                        track_id: clip.track_id,
                        note_id,
                        pitch,
                        velocity: 0.8,
                        length_mus: None,
                    });
                    state.held_key = Some((clip.track_id, note_id, pitch));
                    state.preview_highlights.insert(pitch, f64::INFINITY);
                }
                if state.held_key.is_some() && !ui.input(|input| input.pointer.primary_down()) {
                    if let Some((track_id, note_id, pitch)) = state.held_key.take() {
                        self.emit(UiIntent::ReleasePreviewNote { track_id, note_id });
                        state.preview_highlights.remove(&pitch);
                    }
                }
                draw_content_shading(
                    &clipped,
                    &timeline_grid,
                    content.left(),
                    note_area,
                    display_range,
                );
                timeline_grid.draw(&clipped, note_area);

                let bypass = snap.bypassed(ui.input(|input| input.modifiers));
                let cursor_pos = ui
                    .input(|input| input.pointer.hover_pos())
                    .or_else(|| state.drag.as_ref().map(|drag| drag.origin_pointer));
                let visuals = build_visuals(
                    &midi,
                    &state,
                    &clip,
                    &timeline_grid,
                    note_area,
                    content.left(),
                    state.pitch_scroll,
                    cursor_pos,
                    grid,
                    bypass,
                );

                let bg = ui.interact(
                    note_area,
                    Id::new(("midi-editor-bg", state.clip_id)),
                    Sense::click_and_drag(),
                );
                let hit_note = cursor_pos.is_some_and(|pos| {
                    visuals
                        .iter()
                        .any(|visual| visual.rect.contains(pos) && note_area.contains(pos))
                });
                if bg.double_clicked() && !hit_note {
                    if let Some(pos) = bg
                        .interact_pointer_pos()
                        .or_else(|| ui.input(|input| input.pointer.press_origin()))
                    {
                        let pitch_scroll = state.pitch_scroll;
                        create_note_at_pointer(
                            self,
                            &mut state,
                            &mut midi,
                            &clip,
                            &timeline_grid,
                            note_area,
                            content.left(),
                            pitch_scroll,
                            grid,
                            bypass,
                            pos,
                            doc.meta.tempo_bpm,
                            now,
                        );
                    }
                } else if bg.clicked() && !hit_note {
                    if let Some(pos) = bg
                        .interact_pointer_pos()
                        .or_else(|| ui.input(|input| input.pointer.press_origin()))
                    {
                        let beat = beat_at(pos.x, &timeline_grid, content.left(), bypass);
                        if let Ok(range) = RangeMus::new(beat, beat) {
                            state.select_time(local_time_range(range, &clip, &midi));
                        }
                    }
                }
                if bg.drag_started() && !hit_note {
                    if let Some(pos) = ui
                        .input(|input| input.pointer.press_origin())
                        .or_else(|| bg.interact_pointer_pos())
                    {
                        let beat = beat_at(pos.x, &timeline_grid, content.left(), bypass);
                        if let Ok(range) = RangeMus::new(beat, beat) {
                            state.select_time(local_time_range(range, &clip, &midi));
                        }
                        state.marquee = Some(MarqueeDrag {
                            origin_pointer: pos,
                            current_pointer: pos,
                        });
                    }
                }
                if bg.dragged()
                    && let Some(marquee) = state.marquee
                {
                    let current = bg
                        .interact_pointer_pos()
                        .or_else(|| ui.input(|input| input.pointer.hover_pos()))
                        .unwrap_or(marquee.current_pointer);
                    state.marquee = Some(MarqueeDrag {
                        origin_pointer: marquee.origin_pointer,
                        current_pointer: current,
                    });
                    apply_marquee(
                        &mut state,
                        &visuals,
                        marquee.origin_pointer,
                        current,
                        &timeline_grid,
                        content.left(),
                        note_area,
                        bypass,
                        &clip,
                        &midi,
                    );
                }
                if bg.drag_stopped()
                    && let Some(marquee) = state.marquee.take()
                {
                    let release = bg
                        .interact_pointer_pos()
                        .or_else(|| ui.input(|input| input.pointer.hover_pos()))
                        .unwrap_or(marquee.current_pointer);
                    apply_marquee(
                        &mut state,
                        &visuals,
                        marquee.origin_pointer,
                        release,
                        &timeline_grid,
                        content.left(),
                        note_area,
                        bypass,
                        &clip,
                        &midi,
                    );
                }

                let note_base_color = crate::timeline::clip_color(doc, &clip);
                for visual in &visuals {
                    let note = &visual.note;
                    let selected = state.selected.contains(&note.id);
                    painter.rect_filled(visual.rect, CornerRadius::same(2), note_base_color);
                    painter.rect_stroke(
                        visual.rect,
                        CornerRadius::same(2),
                        Stroke::new(
                            if selected { 1.8_f32 } else { 1.0_f32 },
                            if selected {
                                theme::SELECTION
                            } else {
                                Color32::from_black_alpha(150)
                            },
                        ),
                        StrokeKind::Inside,
                    );
                    let response = ui.interact(
                        visual.rect,
                        Id::new(("midi-note", note.id)),
                        Sense::click_and_drag(),
                    );
                    if response.double_clicked() {
                        delete_note(self, &mut state, &mut midi, note.id);
                    } else if response.clicked() {
                        let additive =
                            ui.input(|input| input.modifiers.command || input.modifiers.shift);
                        state.select_note(note.id, additive);
                        preview_note_for_action(
                            self,
                            &mut state,
                            &midi,
                            clip.track_id,
                            note.id,
                            doc.meta.tempo_bpm,
                            now,
                        );
                    }
                    if response.drag_started()
                        && let Some(pos) = ui
                            .input(|input| input.pointer.press_origin())
                            .or_else(|| response.interact_pointer_pos())
                    {
                        begin_note_drag(&mut state, &midi, note, NoteDragMode::Move, pos);
                    }
                    if response.drag_stopped()
                        && state
                            .drag
                            .as_ref()
                            .is_some_and(|drag| drag.note_id == note.id)
                    {
                        let drag = state.drag.take().expect("drag exists");
                        let preview_id = drag.note_id;
                        let release = response
                            .interact_pointer_pos()
                            .or_else(|| ui.input(|input| input.pointer.hover_pos()))
                            .unwrap_or(drag.origin_pointer);
                        finish_note_drag(
                            self, &mut state, &mut midi, drag, release, ppb, grid, bypass,
                        );
                        preview_note_for_action(
                            self,
                            &mut state,
                            &midi,
                            clip.track_id,
                            preview_id,
                            doc.meta.tempo_bpm,
                            now,
                        );
                    }
                    if visual.rect.width() > 10.0 {
                        for (mode, edge, label) in [
                            (NoteDragMode::ResizeStart, visual.rect.left(), "start"),
                            (NoteDragMode::ResizeEnd, visual.rect.right(), "end"),
                        ] {
                            let handle = Rect::from_center_size(
                                egui::pos2(edge, visual.rect.center().y),
                                Vec2::new(7.0, visual.rect.height().max(10.0)),
                            );
                            let trim = ui.interact(
                                handle,
                                Id::new(("midi-note-trim", note.id, label)),
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
                                begin_note_drag(&mut state, &midi, note, mode, pos);
                            }
                            if trim.drag_stopped()
                                && state
                                    .drag
                                    .as_ref()
                                    .is_some_and(|drag| drag.note_id == note.id)
                            {
                                let drag = state.drag.take().expect("drag exists");
                                let preview_id = drag.note_id;
                                let release = trim
                                    .interact_pointer_pos()
                                    .or_else(|| ui.input(|input| input.pointer.hover_pos()))
                                    .unwrap_or(drag.origin_pointer);
                                finish_note_drag(
                                    self, &mut state, &mut midi, drag, release, ppb, grid, bypass,
                                );
                                preview_note_for_action(
                                    self,
                                    &mut state,
                                    &midi,
                                    clip.track_id,
                                    preview_id,
                                    doc.meta.tempo_bpm,
                                    now,
                                );
                            }
                        }
                    }
                }

                draw_content_shading(
                    &clipped,
                    &timeline_grid,
                    content.left(),
                    note_area,
                    display_range,
                );

                let lanes =
                    Rect::from_min_max(egui::pos2(content.left(), ruler.bottom()), body.max);
                let mut chrome_drag = state.chrome_drag;
                let was_dragging = chrome_drag.is_some();
                let chrome = crate::timeline::draw_clip_chrome(
                    ui,
                    &content_clipped,
                    ClipChrome {
                        grid: timeline_grid,
                        content,
                        ruler,
                        lanes,
                        range: display_range,
                        origin: clip.range,
                        id_salt: Id::new(("midi-editor-clip-edge", state.clip_id)),
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
                state.chrome_drag = chrome_drag;
                if let Some(range) = chrome.preview {
                    state.preview_range = Some(range);
                }
                if was_dragging && state.chrome_drag.is_none() {
                    state.preview_range = None;
                }
                if let Some(range) = chrome.committed {
                    self.emit(UiIntent::SetClipRange {
                        clip_id: state.clip_id,
                        range,
                    });
                }

                if let Some(range) = state
                    .time_selection()
                    .map(|range| absolute_time_range(range, &clip, &midi))
                {
                    crate::timeline::draw_time_selection_fill(
                        &clipped,
                        timeline_grid,
                        content.left(),
                        note_area,
                        range,
                    );
                    let marker_rect = Rect::from_min_max(
                        egui::pos2(content.left(), ruler.top()),
                        egui::pos2(content.right(), note_area.bottom()),
                    );
                    crate::timeline::draw_time_selection_markers(
                        &content_clipped,
                        timeline_grid,
                        content.left(),
                        marker_rect,
                        ruler,
                        range,
                    );
                }

                if let Some(marquee) = state.marquee {
                    let rect = Rect::from_two_pos(marquee.origin_pointer, marquee.current_pointer)
                        .intersect(note_area);
                    if rect.width() > 0.0 || rect.height() > 0.0 {
                        clipped.rect_filled(
                            rect,
                            CornerRadius::ZERO,
                            Color32::from_rgba_unmultiplied(255, 210, 120, 40),
                        );
                        clipped.rect_stroke(
                            rect,
                            CornerRadius::ZERO,
                            Stroke::new(1.0_f32, Color32::WHITE),
                            StrokeKind::Inside,
                        );
                    }
                }

                crate::timeline::draw_playhead(
                    &clipped,
                    note_area,
                    content.left(),
                    timeline_grid,
                    playhead_for_strip(runtime),
                );

                draw_velocity_lane(
                    self,
                    &mut state,
                    &mut midi,
                    doc,
                    &clip,
                    &visuals,
                    ui,
                    velocity,
                    content.left(),
                    &timeline_grid,
                    &painter,
                );

                painter.text(
                    egui::pos2(header.left() + 8.0, velocity.top() + 4.0),
                    Align2::LEFT_TOP,
                    "VELOCITY",
                    FontId::monospace(8.0),
                    theme::TEXT_MUTED,
                );

                let notes_end = midi
                    .notes
                    .iter()
                    .map(|note| {
                        beat_from_content_time(
                            note.start + note.length,
                            clip.range.start,
                            midi.content_start,
                        )
                    })
                    .max()
                    .unwrap_or(clip.range.start);
                let content_end =
                    beat_from_content_time(midi.length, clip.range.start, midi.content_start)
                        .max(notes_end)
                        .max(display_range.end)
                        .max(clip.range.end)
                        + TimeMus::new(4, 1);
                midi_editor_scrollbar(
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
            self.midi_editor = Some(state);
        } else {
            self.close_midi_editor();
        }
    }

    fn midi_editor_placeholder(&mut self, ctx: &egui::Context) {
        let open = crate::views::panel::bottom_panel(
            ctx,
            crate::views::panel::PanelConfig {
                id: "midi_editor_panel",
                resizable: true,
                default_height: 300.0,
                min_height: 100.0,
                max_height: 800.0,
                collapse_height: 160.0,
                handle_id: "midi-editor-panel",
                handle_tooltip: "Drag down to close",
            },
            |ui| {
                ui.label(
                    egui::RichText::new("Select a MIDI clip to edit.").color(theme::TEXT_MUTED),
                );
                true
            },
        );
        if !open {
            self.close_editor();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_visuals(
    midi: &MidiClipData,
    state: &MidiEditorState,
    clip: &ClipEntity,
    grid: &TimelineGrid,
    note_area: Rect,
    content_left: f32,
    pitch_scroll: f32,
    pointer: Option<egui::Pos2>,
    snap_grid: TimeMus,
    bypass: bool,
) -> Vec<NoteVisual> {
    let mut visuals = Vec::with_capacity(midi.notes.len());
    for note in &midi.notes {
        let (span, pitch) = preview_span(note, state, pointer, snap_grid, bypass, midi.length);
        let x1 = grid.x_for_beat(
            beat_from_content_time(span.start, clip.range.start, midi.content_start),
            content_left,
        );
        let x2 = grid.x_for_beat(
            beat_from_content_time(span.end, clip.range.start, midi.content_start),
            content_left,
        );
        let y = y_for_pitch(pitch, note_area.top(), pitch_scroll);
        let full = Rect::from_min_max(
            egui::pos2(x1, y),
            egui::pos2(x2.max(x1 + 2.0), y + ROW_HEIGHT),
        );
        let rect = full.intersect(note_area);
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            continue;
        }
        visuals.push(NoteVisual {
            note: note.clone(),
            span,
            rect,
        });
    }
    visuals
}

#[allow(clippy::too_many_arguments)]
fn preview_span(
    note: &NoteEntity,
    state: &MidiEditorState,
    pointer: Option<egui::Pos2>,
    grid: TimeMus,
    bypass: bool,
    max_length: TimeMus,
) -> (RangeMus, u8) {
    let Some(drag) = state.drag.as_ref() else {
        return (
            RangeMus::new(note.start, note.start + note.length)
                .unwrap_or_else(|_| RangeMus::new(note.start, note.start).unwrap()),
            note.pitch,
        );
    };
    let Some(origin) = drag.origins.iter().find(|origin| origin.id == note.id) else {
        return (
            RangeMus::new(note.start, note.start + note.length)
                .unwrap_or_else(|_| RangeMus::new(note.start, note.start).unwrap()),
            note.pitch,
        );
    };
    let Some(pointer) = pointer else {
        return (
            RangeMus::new(origin.start, origin.start + origin.length)
                .unwrap_or_else(|_| RangeMus::new(origin.start, origin.start).unwrap()),
            origin.pitch,
        );
    };
    let delta = f64::from(pointer.x - drag.origin_pointer.x)
        / f64::from(state.viewport.pixels_per_beat.max(0.001));
    match drag.mode {
        NoteDragMode::Move => {
            let shift = clamp_time_shift(
                snapped_shift(drag.origin_start, delta, grid, bypass),
                &drag.origins,
                max_length,
            );
            let pitches: Vec<u8> = drag.origins.iter().map(|origin| origin.pitch).collect();
            let pitch_shift = clamp_pitch_shift(
                -((pointer.y - drag.origin_pointer.y) / ROW_HEIGHT).round() as i32,
                &pitches,
            );
            let start = origin.start + shift;
            (
                RangeMus::new(start, start + origin.length)
                    .unwrap_or_else(|_| RangeMus::new(origin.start, origin.start).unwrap()),
                (i32::from(origin.pitch) + pitch_shift).clamp(0, 127) as u8,
            )
        }
        NoteDragMode::ResizeStart => {
            let start = (origin.start + snapped_shift(origin.start, delta, grid, bypass))
                .max(TimeMus::zero())
                .min(origin.start + origin.length - TimeMus::new(MIN_NOTE_LEN, MIN_NOTE_DEN));
            (
                RangeMus::new(start, origin.start + origin.length)
                    .unwrap_or_else(|_| RangeMus::new(origin.start, origin.start).unwrap()),
                origin.pitch,
            )
        }
        NoteDragMode::ResizeEnd => {
            let end = (origin.start
                + origin.length
                + snapped_shift(origin.start + origin.length, delta, grid, bypass))
            .min(max_length)
            .max(origin.start + TimeMus::new(MIN_NOTE_LEN, MIN_NOTE_DEN));
            (
                RangeMus::new(origin.start, end)
                    .unwrap_or_else(|_| RangeMus::new(origin.start, origin.start).unwrap()),
                origin.pitch,
            )
        }
    }
}

fn beat_at(x: f32, grid: &TimelineGrid, content_left: f32, bypass: bool) -> TimeMus {
    grid.snap(grid.beat_for_x(x, content_left), bypass)
        .max(TimeMus::zero())
}

#[allow(clippy::too_many_arguments)]
fn create_note_at_pointer(
    ui: &mut NotwigUi,
    state: &mut MidiEditorState,
    midi: &mut MidiClipData,
    clip: &ClipEntity,
    grid: &TimelineGrid,
    note_area: Rect,
    content_left: f32,
    pitch_scroll: f32,
    snap_grid: TimeMus,
    bypass: bool,
    pointer: egui::Pos2,
    tempo_bpm: f64,
    now: f64,
) {
    let beat = beat_at(pointer.x, grid, content_left, bypass);
    let start = content_time_from_beat(beat, clip.range.start, midi.content_start);
    let pitch = pitch_for_y(pointer.y, note_area.top(), pitch_scroll);
    let max_end = start + TimeMus::new(16, 1);
    let end = default_note_end(start, snap_grid, pitch, &midi.notes, max_end);
    let length = end - start;
    if length <= TimeMus::zero() {
        return;
    }
    let note = NoteEntity {
        id: NoteId::from_uuid(Uuid::new_v4()),
        pitch,
        start,
        length,
        velocity: MidiEditorState::default_velocity(),
        release_velocity: None,
        expression: BTreeMap::new(),
    };
    let note_id = note.id;
    create_note_clearing(ui, state, midi, note);
    preview_note_for_action(ui, state, midi, clip.track_id, note_id, tempo_bpm, now);
}

fn delete_note(
    ui: &mut NotwigUi,
    state: &mut MidiEditorState,
    midi: &mut MidiClipData,
    note_id: NoteId,
) {
    let selection = edit::Selection::objects([note_id]);
    let plan = {
        let model = MidiModel::new(midi);
        edit::delete(&model, &selection)
    };
    if let Ok(plan) = plan {
        let lowered = lower_note_plan(midi.length, &midi.notes, plan);
        apply_note_lowered(ui, state, midi, lowered);
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_marquee(
    state: &mut MidiEditorState,
    visuals: &[NoteVisual],
    origin: egui::Pos2,
    current: egui::Pos2,
    grid: &TimelineGrid,
    content_left: f32,
    note_area: Rect,
    bypass: bool,
    clip: &ClipEntity,
    midi: &MidiClipData,
) {
    let origin_beat = beat_at(origin.x, grid, content_left, bypass);
    let current_beat = beat_at(current.x, grid, content_left, bypass);
    let (start, end) = if origin_beat <= current_beat {
        (origin_beat, current_beat)
    } else {
        (current_beat, origin_beat)
    };
    let Ok(absolute) = RangeMus::new(start, end) else {
        return;
    };
    let local = local_time_range(absolute, clip, midi);
    let rect = Rect::from_two_pos(origin, current).intersect(note_area);
    let selected: Vec<&NoteVisual> = visuals
        .iter()
        .filter(|visual| visual.rect.intersects(rect))
        .collect();
    let ids: Vec<NoteId> = selected.iter().map(|visual| visual.note.id).collect();
    let mut start = local.start;
    let mut end = local.end;
    for visual in &selected {
        start = start.min(visual.span.start);
        end = end.max(visual.span.end);
    }
    let expanded = RangeMus::new(start.min(end), end.max(start)).unwrap_or(local);
    state.select_marquee(ids, expanded);
}

fn absolute_time_range(range: RangeMus, clip: &ClipEntity, midi: &MidiClipData) -> RangeMus {
    RangeMus::new(
        beat_from_content_time(range.start, clip.range.start, midi.content_start),
        beat_from_content_time(range.end, clip.range.start, midi.content_start),
    )
    .unwrap_or(range)
}

fn local_time_range(range: RangeMus, clip: &ClipEntity, midi: &MidiClipData) -> RangeMus {
    RangeMus::new(
        content_time_from_beat(range.start, clip.range.start, midi.content_start),
        content_time_from_beat(range.end, clip.range.start, midi.content_start),
    )
    .unwrap_or(range)
}

fn begin_note_drag(
    state: &mut MidiEditorState,
    midi: &MidiClipData,
    note: &NoteEntity,
    mode: NoteDragMode,
    pointer: egui::Pos2,
) {
    if mode != NoteDragMode::Move || !state.selected.contains(&note.id) {
        state.select_one(note.id);
    }
    let origins: Vec<NoteOrigin> = if mode == NoteDragMode::Move {
        midi.notes
            .iter()
            .filter(|candidate| state.selected.contains(&candidate.id))
            .map(|candidate| NoteOrigin {
                id: candidate.id,
                start: candidate.start,
                length: candidate.length,
                pitch: candidate.pitch,
            })
            .collect()
    } else {
        vec![NoteOrigin {
            id: note.id,
            start: note.start,
            length: note.length,
            pitch: note.pitch,
        }]
    };
    state.drag = Some(NoteDrag {
        note_id: note.id,
        mode,
        origin_pointer: pointer,
        origin_start: note.start,
        origin_length: note.length,
        origins,
    });
}

#[allow(clippy::too_many_arguments)]
fn finish_note_drag(
    ui: &mut NotwigUi,
    state: &mut MidiEditorState,
    midi: &mut MidiClipData,
    drag: NoteDrag,
    release: egui::Pos2,
    ppb: f32,
    grid: TimeMus,
    bypass: bool,
) {
    let delta = f64::from(release.x - drag.origin_pointer.x) / f64::from(ppb.max(0.001));
    let model = MidiModel::new(midi);
    let plan = match drag.mode {
        NoteDragMode::Move => {
            let shift = clamp_time_shift(
                snapped_shift(drag.origin_start, delta, grid, bypass),
                &drag.origins,
                midi.length,
            );
            let pitches: Vec<u8> = drag.origins.iter().map(|origin| origin.pitch).collect();
            let pitch_shift = clamp_pitch_shift(
                -((release.y - drag.origin_pointer.y) / ROW_HEIGHT).round() as i32,
                &pitches,
            );
            let moves: Vec<edit::ObjectMove<Pitch, NoteId>> = drag
                .origins
                .iter()
                .filter_map(|origin| {
                    let start = origin.start + shift;
                    let end = start + origin.length;
                    let lane = Pitch((i32::from(origin.pitch) + pitch_shift).clamp(0, 127) as u8);
                    RangeMus::new(start, end).ok().map(|span| edit::ObjectMove {
                        object: origin.id,
                        lane,
                        span,
                    })
                })
                .collect();
            edit::relocate_objects(&model, &moves)
        }
        NoteDragMode::ResizeStart => {
            let target = drag.origin_start + snapped_shift(drag.origin_start, delta, grid, bypass);
            edit::trim_object(&model, drag.note_id, edit::Edge::Start, target)
        }
        NoteDragMode::ResizeEnd => {
            let target = drag.origin_start
                + drag.origin_length
                + snapped_shift(drag.origin_start + drag.origin_length, delta, grid, bypass);
            edit::trim_object(&model, drag.note_id, edit::Edge::End, target)
        }
    };
    if let Ok(plan) = plan {
        let lowered = lower_note_plan(midi.length, &midi.notes, plan);
        apply_note_lowered(ui, state, midi, lowered);
    }
    state.drag = None;
}

fn played_pitches(midi: &MidiClipData, clip: &ClipEntity, runtime: &RuntimeView) -> BTreeSet<u8> {
    let mut pitches = BTreeSet::new();
    if !runtime.playing {
        return pitches;
    }
    for note in &midi.notes {
        let start = beat_from_content_time(note.start, clip.range.start, midi.content_start);
        let end = beat_from_content_time(
            note.start + note.length,
            clip.range.start,
            midi.content_start,
        );
        if runtime.playhead >= start && runtime.playhead < end {
            pitches.insert(note.pitch);
        }
    }
    pitches
}

fn preview_note_for_action(
    ui: &mut NotwigUi,
    state: &mut MidiEditorState,
    midi: &MidiClipData,
    track_id: TrackId,
    note_id: NoteId,
    tempo_bpm: f64,
    now: f64,
) {
    let Some(note) = midi.notes.iter().find(|note| note.id == note_id) else {
        return;
    };
    let seconds = note.length.as_f64() * 60.0 / tempo_bpm.max(1.0);
    state
        .preview_highlights
        .insert(note.pitch, now + seconds.max(0.08));
    ui.emit(UiIntent::PreviewNote {
        track_id,
        note_id,
        pitch: note.pitch,
        velocity: note.velocity.0,
        length_mus: Some(note.length),
    });
}

fn velocity_preview(
    note: &NoteEntity,
    state: &MidiEditorState,
    pointer: Option<egui::Pos2>,
    velocity_rect: Rect,
) -> Norm01 {
    let Some(drag) = state.velocity_drag else {
        return note.velocity;
    };
    if drag.note_id != note.id {
        return note.velocity;
    }
    let Some(pointer) = pointer else {
        return Norm01::clamped(drag.origin_velocity);
    };
    velocity_for_y(pointer.y, velocity_rect.top(), velocity_rect.height())
}

#[allow(clippy::too_many_arguments)]
fn draw_velocity_lane(
    ui: &mut NotwigUi,
    state: &mut MidiEditorState,
    midi: &mut MidiClipData,
    doc: &ProjectDoc,
    clip: &ClipEntity,
    visuals: &[NoteVisual],
    egui_ui: &mut egui::Ui,
    velocity: Rect,
    content_left: f32,
    grid: &TimelineGrid,
    painter: &egui::Painter,
) {
    painter.line_segment(
        [
            egui::pos2(velocity.left(), velocity.top()),
            egui::pos2(velocity.right(), velocity.top()),
        ],
        Stroke::new(1.0_f32, theme::GRID_STRONG),
    );
    let color = crate::timeline::clip_color(doc, clip);
    let cursor_pos = egui_ui.input(|input| input.pointer.hover_pos());
    for visual in visuals {
        let note = &visual.note;
        let preview = velocity_preview(note, state, cursor_pos, velocity);
        let x = grid
            .x_for_beat(
                beat_from_content_time(visual.span.start, clip.range.start, midi.content_start),
                content_left,
            )
            .clamp(velocity.left() + 2.0, velocity.right() - 2.0);
        let y = velocity_y(preview.0, velocity.top(), velocity.height());
        let selected = state.selected.contains(&note.id);
        let width = if selected { 2.5_f32 } else { 1.5_f32 };
        painter.line_segment(
            [egui::pos2(x, velocity.bottom()), egui::pos2(x, y)],
            Stroke::new(width, color),
        );
        painter.circle_filled(
            egui::pos2(x, y),
            if selected { 4.0_f32 } else { 3.0_f32 },
            color,
        );
        let hit = Rect::from_min_max(
            egui::pos2(x - 6.0, velocity.top()),
            egui::pos2(x + 6.0, velocity.bottom()),
        )
        .intersect(velocity);
        if hit.height() <= 0.0 {
            continue;
        }
        let response = egui_ui.interact(
            hit,
            Id::new(("midi-velocity", note.id)),
            Sense::click_and_drag(),
        );
        if response.drag_started() {
            state.select_one(note.id);
            state.velocity_drag = Some(VelocityDrag {
                note_id: note.id,
                origin_velocity: note.velocity.0,
            });
        }
        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            let value = velocity_for_y(pos.y, velocity.top(), velocity.height());
            set_note_velocity(ui, state, midi, note.id, value);
        }
        if response.drag_stopped()
            && let Some(drag) = state.velocity_drag.take()
        {
            let pos = response
                .interact_pointer_pos()
                .or_else(|| egui_ui.input(|input| input.pointer.hover_pos()));
            if let Some(pos) = pos {
                let value = velocity_for_y(pos.y, velocity.top(), velocity.height());
                set_note_velocity(ui, state, midi, drag.note_id, value);
            }
        }
    }
    painter.text(
        egui::pos2(velocity.left() + 4.0, velocity.bottom() - 4.0),
        Align2::LEFT_BOTTOM,
        "0",
        FontId::monospace(8.0),
        theme::TEXT_MUTED,
    );
    painter.text(
        egui::pos2(velocity.left() + 4.0, velocity.top() + 14.0),
        Align2::LEFT_TOP,
        "1",
        FontId::monospace(8.0),
        theme::TEXT_MUTED,
    );
}

fn set_note_velocity(
    ui: &mut NotwigUi,
    state: &mut MidiEditorState,
    midi: &mut MidiClipData,
    note_id: NoteId,
    value: Norm01,
) {
    let Some(note) = midi.notes.iter_mut().find(|note| note.id == note_id) else {
        return;
    };
    if note.velocity == value {
        return;
    }
    note.velocity = value;
    let updated = note.clone();
    ui.emit(UiIntent::EditMidiNotes {
        clip_id: state.clip_id,
        notes: vec![updated],
        removed: Vec::new(),
    });
}

#[allow(clippy::too_many_arguments)]
fn draw_pitch_rows(
    ui: &mut egui::Ui,
    clipped: &egui::Painter,
    painter: &egui::Painter,
    header: Rect,
    note_area: Rect,
    scroll: f32,
    highlights: &BTreeMap<u8, f64>,
    playing: &BTreeSet<u8>,
    now: f64,
) -> Option<u8> {
    let key_clip = Rect::from_min_max(
        egui::pos2(header.left(), note_area.top()),
        egui::pos2(header.right(), note_area.bottom()),
    );
    let keys = painter.with_clip_rect(key_clip);
    let mut clicked = None;
    let first = scroll.floor() as i32;
    let last = (scroll + note_area.height() / ROW_HEIGHT).ceil() as i32 + 1;
    for row in first..=last {
        let pitch = 127 - row;
        if !(0..=127).contains(&pitch) {
            continue;
        }
        let y = note_area.top() + (row as f32 - scroll) * ROW_HEIGHT;
        let pitch_u8 = u8::try_from(pitch).unwrap_or(0);
        let black = is_black_key(pitch_u8);
        let row_rect = Rect::from_min_max(
            egui::pos2(note_area.left(), y),
            egui::pos2(note_area.right(), y + ROW_HEIGHT),
        );
        clipped.rect_filled(
            row_rect,
            CornerRadius::ZERO,
            if black {
                Color32::from_rgb(22, 24, 29)
            } else {
                theme::LANE
            },
        );
        let key_rect = Rect::from_min_max(
            egui::pos2(header.left(), y),
            egui::pos2(header.right(), y + ROW_HEIGHT),
        );
        keys.rect_filled(
            key_rect,
            CornerRadius::ZERO,
            if black {
                Color32::from_rgb(32, 35, 41)
            } else {
                Color32::from_rgb(200, 204, 210)
            },
        );
        let highlighted = highlights
            .get(&pitch_u8)
            .is_some_and(|expiry| !expiry.is_finite() || *expiry > now)
            || playing.contains(&pitch_u8);
        if highlighted {
            keys.rect_filled(
                key_rect,
                CornerRadius::ZERO,
                Color32::from_rgba_unmultiplied(255, 166, 66, 150),
            );
        }
        painter.line_segment(
            [
                egui::pos2(header.left(), y),
                egui::pos2(note_area.right(), y),
            ],
            Stroke::new(0.5_f32, theme::GRID),
        );
        let label = (pitch_u8 % 12 == 0).then(|| format!("C{}", i32::from(pitch_u8) / 12 - 1));
        if let Some(label) = label {
            keys.text(
                egui::pos2(header.left() + 6.0, y + ROW_HEIGHT * 0.5),
                Align2::LEFT_CENTER,
                label,
                FontId::monospace(8.0),
                Color32::from_black_alpha(210),
            );
        }
        let hit = key_rect.intersect(key_clip);
        if hit.height() <= 0.0 {
            continue;
        }
        let response = ui.interact(
            hit,
            Id::new(("midi-key", pitch_u8)),
            Sense::click_and_drag(),
        );
        if response.is_pointer_button_down_on() {
            clicked = Some(pitch_u8);
        }
    }
    clicked
}

fn is_black_key(pitch: u8) -> bool {
    matches!(pitch % 12, 1 | 3 | 6 | 8 | 10)
}

fn draw_content_shading(
    clipped: &egui::Painter,
    grid: &TimelineGrid,
    content_left: f32,
    note_area: Rect,
    range: RangeMus,
) {
    let start_x = grid.x_for_beat(range.start, content_left);
    let end_x = grid.x_for_beat(range.end, content_left);
    if start_x > note_area.left() {
        clipped.rect_filled(
            Rect::from_min_max(note_area.min, egui::pos2(start_x, note_area.bottom())),
            CornerRadius::ZERO,
            Color32::from_black_alpha(140),
        );
    }
    if end_x < note_area.right() {
        clipped.rect_filled(
            Rect::from_min_max(egui::pos2(end_x, note_area.top()), note_area.max),
            CornerRadius::ZERO,
            Color32::from_black_alpha(140),
        );
    }
}
