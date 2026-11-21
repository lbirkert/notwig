use notwig_core::edit;
use notwig_core::{ClipEntity, MidiClipData, RangeMus, TimeMus};

use crate::commands::{KeyChord, ShortcutCommand};
use crate::intent::UiIntent;
use crate::shell::NotwigUi;
use crate::state::EditFocus;
use crate::widgets::snap_time;

use super::geometry::{beat_from_content_time, clamp_pitch_shift, content_time_from_beat};
use super::model::Pitch;
use super::model::{MidiModel, apply_note_lowered, lower_note_plan};
use super::state::MidiEditorState;
use crate::views::arranger::edit::paste_target;

pub(super) fn handle_midi_editor_actions(
    ui: &mut NotwigUi,
    ctx: &egui::Context,
    state: &mut MidiEditorState,
    midi: &mut MidiClipData,
    clip: &ClipEntity,
    grid: TimeMus,
    playhead: TimeMus,
    tempo_bpm: f64,
) {
    if ui.edit_focus != EditFocus::MidiEditor {
        return;
    }
    if ui.shortcut_editor.capture.is_some() {
        return;
    }
    if ctx.wants_keyboard_input() {
        return;
    }
    sync_global_time_selection(ui, state, clip, midi);
    state.sync_notes(&midi.notes);
    let (copy_native, cut_native, paste_native) = ctx.input(|input| {
        (
            input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::Copy)),
            input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::Cut)),
            input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::Paste(_))),
        )
    });
    let copy = ui.shortcuts.was_pressed(ctx, ShortcutCommand::Copy)
        || (copy_native
            && ui
                .shortcuts
                .has_binding(ShortcutCommand::Copy, KeyChord::command(egui::Key::C)));
    let cut = ui.shortcuts.was_pressed(ctx, ShortcutCommand::Cut)
        || (cut_native
            && ui
                .shortcuts
                .has_binding(ShortcutCommand::Cut, KeyChord::command(egui::Key::X)));
    let paste = ui.shortcuts.was_pressed(ctx, ShortcutCommand::Paste)
        || (paste_native
            && ui
                .shortcuts
                .has_binding(ShortcutCommand::Paste, KeyChord::command(egui::Key::V)));
    let duplicate = ui.shortcuts.was_pressed(ctx, ShortcutCommand::Duplicate);
    let delete = ui.shortcuts.was_pressed(ctx, ShortcutCommand::Delete);
    let selection = state.selection();

    if !selection.objects.is_empty() {
        move_selection_with_arrows(
            ui,
            ctx,
            state,
            midi,
            grid,
            &selection,
            clip.track_id,
            tempo_bpm,
        );
    }

    if !selection.is_empty() {
        if cut {
            let pair = {
                let model = MidiModel::new(midi);
                edit::cut(&model, &selection)
            };
            if let Ok((extract, plan)) = pair {
                let lowered = lower_note_plan(midi.length, &midi.notes, plan);
                apply_note_lowered(ui, state, midi, lowered);
                state.clipboard.extract = Some(extract);
                ctx.copy_text("notwig-midi-notes".into());
            }
        } else {
            if copy {
                let extract = {
                    let model = MidiModel::new(midi);
                    edit::copy(&model, &selection)
                };
                if let Ok(extract) = extract {
                    state.clipboard.extract = Some(extract);
                    ctx.copy_text("notwig-midi-notes".into());
                }
            }
            if delete {
                let plan = {
                    let model = MidiModel::new(midi);
                    edit::delete(&model, &selection)
                };
                if let Ok(plan) = plan {
                    let lowered = lower_note_plan(midi.length, &midi.notes, plan);
                    apply_note_lowered(ui, state, midi, lowered);
                }
            }
        }
    }

    if paste && let Some(extract) = state.clipboard.extract.clone() {
        let at = if let Some(range) = state.time_selection() {
            range.start
        } else {
            let beat = paste_target(None, playhead);
            let at = content_time_from_beat(beat, clip.range.start, midi.content_start);
            snap_time(at.max(TimeMus::zero()), grid, !ui.state.snap.enabled)
        };
        let plan = {
            let model = MidiModel::new(midi);
            edit::paste(&model, &extract, at)
        };
        if let Ok(plan) = plan {
            let lowered = lower_note_plan(midi.length, &midi.notes, plan);
            apply_note_lowered(ui, state, midi, lowered);
        }
    }

    if duplicate && !selection.is_empty() {
        let mut duplicate_shift = None;
        let plan = if selection.objects.is_empty() {
            let model = MidiModel::new(midi);
            edit::duplicate(&model, &selection)
        } else {
            let model = MidiModel::new(midi);
            let extract = edit::copy(&model, &selection);
            let bounding_start = selection
                .objects
                .iter()
                .filter_map(|id| midi.notes.iter().find(|note| note.id == *id))
                .map(|note| note.start)
                .min();
            match (extract, bounding_start) {
                (Ok(extract), Some(bounding_start)) => {
                    let offset = state
                        .time_selection()
                        .map_or(extract.length, RangeMus::length);
                    duplicate_shift = Some(offset);
                    let at = bounding_start + offset;
                    edit::paste(&model, &extract, at)
                }
                (Err(err), _) => Err(err),
                (Ok(_), None) => edit::duplicate(&model, &selection),
            }
        };
        if let Ok(plan) = plan {
            let lowered = lower_note_plan(midi.length, &midi.notes, plan);
            apply_note_lowered(ui, state, midi, lowered);
            if let Some(offset) = duplicate_shift {
                state.shift_time_selection(offset);
            }
        }
    }
}

fn sync_global_time_selection(
    ui: &mut NotwigUi,
    state: &MidiEditorState,
    clip: &ClipEntity,
    midi: &MidiClipData,
) {
    let mut start: Option<TimeMus> = None;
    let mut end: Option<TimeMus> = None;
    for id in state.selected_ids() {
        let span = state.note_spans.get(&id).copied().or_else(|| {
            midi.notes
                .iter()
                .find(|note| note.id == id)
                .and_then(|note| RangeMus::new(note.start, note.start + note.length).ok())
        });
        let Some(span) = span else {
            continue;
        };
        start = Some(start.map_or(span.start, |value| value.min(span.start)));
        end = Some(end.map_or(span.end, |value| value.max(span.end)));
    }
    let (Some(start), Some(end)) = (start, end) else {
        return;
    };
    let Ok(local) = RangeMus::new(start, end) else {
        return;
    };
    let Ok(absolute) = RangeMus::new(
        beat_from_content_time(local.start, clip.range.start, midi.content_start),
        beat_from_content_time(local.end, clip.range.start, midi.content_start),
    ) else {
        return;
    };
    let unchanged = ui.state.selection.time.as_ref().is_some_and(|time| {
        time.range == absolute
            && time.tracks.len() == 1
            && time.contains_track(clip.track_id)
            && ui.state.selection.clips.contains(&clip.id)
    });
    if !unchanged {
        ui.state.selection.select_time(absolute, &[clip.track_id]);
        ui.state.selection.select_clip(clip.id, clip.track_id, true);
    }
}

fn move_selection_with_arrows(
    ui: &mut NotwigUi,
    ctx: &egui::Context,
    state: &mut MidiEditorState,
    midi: &mut MidiClipData,
    grid: TimeMus,
    selection: &edit::Selection<Pitch, notwig_core::NoteId>,
    track_id: notwig_core::TrackId,
    tempo_bpm: f64,
) {
    let (left, right, up, down, shift) = ctx.input(|input| {
        (
            input.key_pressed(egui::Key::ArrowLeft),
            input.key_pressed(egui::Key::ArrowRight),
            input.key_pressed(egui::Key::ArrowUp),
            input.key_pressed(egui::Key::ArrowDown),
            input.modifiers.shift,
        )
    });
    if !left && !right && !up && !down {
        return;
    }

    let model = MidiModel::new(midi);
    let plan = if left || right {
        let delta = if left { TimeMus::zero() - grid } else { grid };
        edit::move_selection(&model, selection, delta)
    } else {
        let semitones = if shift { 12 } else { 1 };
        let requested = if up { semitones } else { -semitones };
        let pitches: Vec<u8> = selection
            .objects
            .iter()
            .filter_map(|id| midi.notes.iter().find(|note| note.id == *id))
            .map(|note| note.pitch)
            .collect();
        let shift = clamp_pitch_shift(requested, &pitches);
        let moves: Vec<edit::ObjectMove<Pitch, notwig_core::NoteId>> = selection
            .objects
            .iter()
            .filter_map(|id| midi.notes.iter().find(|note| note.id == *id))
            .filter_map(|note| {
                let lane = Pitch((i32::from(note.pitch) + shift).clamp(0, 127) as u8);
                RangeMus::new(note.start, note.start + note.length)
                    .ok()
                    .map(|span| edit::ObjectMove {
                        object: note.id,
                        lane,
                        span,
                    })
            })
            .collect();
        edit::relocate_objects(&model, &moves)
    };
    if let Ok(plan) = plan {
        let lowered = lower_note_plan(midi.length, &midi.notes, plan);
        apply_note_lowered(ui, state, midi, lowered);
        if left || right {
            let delta = if left { TimeMus::zero() - grid } else { grid };
            state.shift_time_selection(delta);
        }
        let now = ctx.input(|input| input.time);
        for note_id in &selection.objects {
            preview_note_after_move(ui, state, midi, track_id, *note_id, tempo_bpm, now);
        }
    }
}

fn preview_note_after_move(
    ui: &mut NotwigUi,
    state: &mut MidiEditorState,
    midi: &MidiClipData,
    track_id: notwig_core::TrackId,
    note_id: notwig_core::NoteId,
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
