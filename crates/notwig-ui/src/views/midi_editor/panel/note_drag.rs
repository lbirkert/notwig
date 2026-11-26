use std::collections::BTreeMap;

use egui::Rect;
use notwig_core::edit;
use notwig_core::{ClipEntity, MidiClipData, NoteEntity, NoteId, RangeMus, TimeMus};
use uuid::Uuid;

use super::super::geometry::{
    ROW_HEIGHT, beat_from_content_time, clamp_pitch_shift, clamp_time_shift,
    content_time_from_beat, pitch_for_y, snapped_shift,
};
use super::super::model::{
    MidiModel, Pitch, apply_note_lowered, create_note_clearing, default_note_end, lower_note_plan,
};
use super::super::state::{MidiEditorState, NoteDrag, NoteDragMode, NoteOrigin};
use super::audition::preview_note_for_action;
use super::paint::NoteVisual;
use crate::shell::NotwigUi;
use crate::timeline::TimelineGrid;

#[allow(clippy::too_many_arguments)]
pub(super) fn create_note_at_pointer(
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

pub(super) fn delete_note(
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
pub(super) fn apply_marquee(
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

pub(super) fn absolute_time_range(
    range: RangeMus,
    clip: &ClipEntity,
    midi: &MidiClipData,
) -> RangeMus {
    RangeMus::new(
        beat_from_content_time(range.start, clip.range.start, midi.content_start),
        beat_from_content_time(range.end, clip.range.start, midi.content_start),
    )
    .unwrap_or(range)
}

pub(super) fn local_time_range(
    range: RangeMus,
    clip: &ClipEntity,
    midi: &MidiClipData,
) -> RangeMus {
    RangeMus::new(
        content_time_from_beat(range.start, clip.range.start, midi.content_start),
        content_time_from_beat(range.end, clip.range.start, midi.content_start),
    )
    .unwrap_or(range)
}

pub(super) fn begin_note_drag(
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
pub(super) fn finish_note_drag(
    ui: &mut NotwigUi,
    state: &mut MidiEditorState,
    midi: &mut MidiClipData,
    drag: NoteDrag,
    release: egui::Pos2,
    ppb: f32,
    grid: TimeMus,
    bypass: bool,
    offset: TimeMus,
) {
    let delta = f64::from(release.x - drag.origin_pointer.x) / f64::from(ppb.max(0.001));
    let model = MidiModel::new(midi);
    let plan = match drag.mode {
        NoteDragMode::Move => {
            let shift = clamp_time_shift(
                snapped_shift(drag.origin_start, offset, delta, grid, bypass),
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
            let target =
                drag.origin_start + snapped_shift(drag.origin_start, offset, delta, grid, bypass);
            edit::trim_object(&model, drag.note_id, edit::Edge::Start, target)
        }
        NoteDragMode::ResizeEnd => {
            let target = drag.origin_start
                + drag.origin_length
                + snapped_shift(
                    drag.origin_start + drag.origin_length,
                    offset,
                    delta,
                    grid,
                    bypass,
                );
            edit::trim_object(&model, drag.note_id, edit::Edge::End, target)
        }
    };
    if let Ok(plan) = plan {
        let lowered = lower_note_plan(midi.length, &midi.notes, plan);
        apply_note_lowered(ui, state, midi, lowered);
    }
    state.drag = None;
}

pub(super) fn beat_at(x: f32, grid: &TimelineGrid, content_left: f32, bypass: bool) -> TimeMus {
    grid.snap(grid.beat_for_x(x, content_left), bypass)
}
