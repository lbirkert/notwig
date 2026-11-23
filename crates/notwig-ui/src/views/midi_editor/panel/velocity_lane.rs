use egui::{Align2, FontId, Id, Rect, Sense, Stroke};
use notwig_core::{ClipEntity, MidiClipData, Norm01, NoteEntity, ProjectDoc};

use super::super::geometry::{beat_from_content_time, velocity_for_y, velocity_y};
use super::super::state::{MidiEditorState, VelocityDrag};
use crate::intent::UiIntent;
use crate::shell::NotwigUi;
use crate::theme;
use crate::timeline::TimelineGrid;

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
pub(super) fn draw_velocity_lane(
    ui: &mut NotwigUi,
    state: &mut MidiEditorState,
    midi: &mut MidiClipData,
    doc: &ProjectDoc,
    clip: &ClipEntity,
    visuals: &[super::paint::NoteVisual],
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
    note_id: notwig_core::NoteId,
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
