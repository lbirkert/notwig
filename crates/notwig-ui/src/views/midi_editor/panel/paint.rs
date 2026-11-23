use std::collections::{BTreeMap, BTreeSet};

use egui::{Align2, Color32, CornerRadius, FontId, Id, Rect, Sense, Stroke};
use notwig_core::{ClipEntity, MidiClipData, NoteEntity, RangeMus, TimeMus};

use super::super::geometry::{
    ROW_HEIGHT, absolute_offset, beat_from_content_time, clamp_pitch_shift, clamp_time_shift,
    snapped_shift, y_for_pitch,
};
use super::super::state::{MidiEditorState, NoteDragMode};
use crate::theme;
use crate::timeline::TimelineGrid;

const MIN_NOTE_LEN: i64 = 1;
const MIN_NOTE_DEN: u32 = 16;

#[derive(Debug, Clone)]
pub(super) struct NoteVisual {
    pub(super) note: NoteEntity,
    pub(super) span: RangeMus,
    pub(super) rect: Rect,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn build_visuals(
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
    let offset = absolute_offset(clip.range.start, midi.content_start);
    let mut visuals = Vec::with_capacity(midi.notes.len());
    for note in &midi.notes {
        let (span, pitch) =
            preview_span(note, state, pointer, snap_grid, bypass, midi.length, offset);
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
    offset: TimeMus,
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
                snapped_shift(drag.origin_start, offset, delta, grid, bypass),
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
            let start = (origin.start + snapped_shift(origin.start, offset, delta, grid, bypass))
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
                + snapped_shift(origin.start + origin.length, offset, delta, grid, bypass))
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

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_pitch_rows(
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
    // Rows increase downward while MIDI pitch increases upward: row 0 is
    // pitch 127, so pitch = 127 - row. Fetch one extra row past the edges to
    // cover partially visible rows.
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

pub(super) fn draw_content_shading(
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
            Color32::from_black_alpha(100),
        );
    }
    if end_x < note_area.right() {
        clipped.rect_filled(
            Rect::from_min_max(egui::pos2(end_x, note_area.top()), note_area.max),
            CornerRadius::ZERO,
            Color32::from_black_alpha(100),
        );
    }
}
