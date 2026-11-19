use notwig_core::edit;
use notwig_core::{ClipEntity, MidiClipData, TimeMus};

use crate::commands::{KeyChord, ShortcutCommand};
use crate::shell::NotwigUi;
use crate::state::EditFocus;
use crate::widgets::snap_time;

use super::geometry::content_time_from_beat;
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
        let at = at
            .max(TimeMus::zero())
            .min((midi.length - extract.length).max(TimeMus::zero()));
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
        let plan = {
            let model = MidiModel::new(midi);
            edit::duplicate(&model, &selection)
        };
        if let Ok(plan) = plan {
            let lowered = lower_note_plan(midi.length, &midi.notes, plan);
            apply_note_lowered(ui, state, midi, lowered);
        }
    }
}
