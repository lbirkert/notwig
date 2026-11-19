use notwig_core::edit;
use notwig_core::{ClipEntity, MidiClipData, RangeMus, TimeMus};

use crate::commands::{KeyChord, ShortcutCommand};
use crate::shell::NotwigUi;
use crate::state::EditFocus;
use crate::widgets::snap_time;

use super::geometry::{clamp_pitch_shift, content_time_from_beat};
use super::model::Pitch;
use super::model::{MidiModel, apply_note_lowered, lower_note_plan};
use super::state::PianoRollState;
use crate::views::arranger::edit::paste_target;

pub(super) fn handle_piano_roll_actions(
    ui: &mut NotwigUi,
    ctx: &egui::Context,
    state: &mut PianoRollState,
    midi: &mut MidiClipData,
    clip: &ClipEntity,
    grid: TimeMus,
    playhead: TimeMus,
) {
    if ui.edit_focus != EditFocus::PianoRoll {
        return;
    }
    if ui.shortcut_editor.capture.is_some() {
        return;
    }
    if ctx.wants_keyboard_input() {
        return;
    }
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
        move_selection_with_arrows(ui, ctx, state, midi, grid, &selection);
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
            }
        } else {
            if copy {
                let extract = {
                    let model = MidiModel::new(midi);
                    edit::copy(&model, &selection)
                };
                if let Ok(extract) = extract {
                    state.clipboard.extract = Some(extract);
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
            snap_time(at.max(TimeMus::zero()), grid, false)
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
        }
    }
}

fn move_selection_with_arrows(
    ui: &mut NotwigUi,
    ctx: &egui::Context,
    state: &mut PianoRollState,
    midi: &mut MidiClipData,
    grid: TimeMus,
    selection: &edit::Selection<Pitch, notwig_core::NoteId>,
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
    }
}
