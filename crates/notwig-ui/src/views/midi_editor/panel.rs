use egui::{Align2, Color32, CornerRadius, FontId, Id, Rect, Sense, Stroke, StrokeKind, Vec2};
use notwig_core::{ClipPayload, NoteId, ProjectDoc, RangeMus, TimeMus};
use uuid::Uuid;

use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::shell::NotwigUi;
use crate::state::Viewport;
use crate::theme;
use crate::timeline::{BeatStrip, ClipChrome, TimelineGrid, playhead_for_strip};

mod audition;
mod note_drag;
mod paint;
mod velocity_lane;

use audition::{played_pitches, preview_note_for_action};
use note_drag::{
    absolute_time_range, apply_marquee, beat_at, begin_note_drag, create_note_at_pointer,
    delete_note, finish_note_drag, local_time_range,
};
use paint::{build_visuals, draw_content_shading, draw_pitch_rows};
use velocity_lane::draw_velocity_lane;

use super::actions::handle_midi_editor_actions;
use super::geometry::{absolute_offset, beat_from_content_time, clamp_pitch_scroll};
use super::navigation::{SCROLLBAR_H, midi_editor_navigation, midi_editor_scrollbar};
use super::state::{MarqueeDrag, NoteDragMode};

const KEY_WIDTH: f32 = 56.0;
pub(super) const RULER_H: f32 = 38.0;
const VELOCITY_H: f32 = 62.0;

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
            // Row 0 is pitch 127 at the top. Scrolling by `127 - average - 8`
            // places the average pitch a few rows down from the top so notes are
            // in view; the per-frame clamp tightens this to the real viewport.
            state.pitch_scroll = (127.0 - f32::from(average) - 8.0).clamp(0.0, 96.0);
            state.initialized = true;
        }
        state.sync_notes(&midi.notes);
        let display_range = state.preview_range.unwrap_or(clip.range);
        let ppb = state
            .viewport
            .pixels_per_beat
            .clamp(Viewport::MIN_PPB, Viewport::MAX_PPB);
        let bar_length = doc.meta.bar_length();
        let beat_length = doc.meta.beat_length();
        let mut snap = self.state.snap;
        let grid = snap.step(ppb, bar_length);

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

        let open = crate::views::panel::bottom_panel_styled(
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
            theme::PANEL,
            egui::Margin::same(0),
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
                let grid = snap.step(ppb, bar_length);

                let painter = ui.painter().clone();
                painter.rect_filled(rect, CornerRadius::ZERO, theme::PANEL);
                painter.rect_filled(header, CornerRadius::ZERO, theme::PANEL_LIGHT);
                painter.rect_filled(note_area, CornerRadius::ZERO, theme::LANE);
                painter.rect_filled(velocity, CornerRadius::ZERO, theme::PANEL_LIGHT);

                let timeline_grid =
                    TimelineGrid::new(state.viewport.start, ppb, bar_length, beat_length, snap);
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
                let content_offset = absolute_offset(clip.range.start, midi.content_start);
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
                            self,
                            &mut state,
                            &mut midi,
                            drag,
                            release,
                            ppb,
                            grid,
                            bypass,
                            content_offset,
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
                                    self,
                                    &mut state,
                                    &mut midi,
                                    drag,
                                    release,
                                    ppb,
                                    grid,
                                    bypass,
                                    content_offset,
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
                    bar_length,
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
