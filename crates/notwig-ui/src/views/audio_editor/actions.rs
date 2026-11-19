use notwig_core::edit::{self, Plan, Selection};
use notwig_core::sequencer::{AudioClipData, AudioSlice};
use notwig_core::{RangeMus, SliceId, TempoMap, TimeMus};

use crate::Clipboard;
use crate::commands::{KeyChord, ShortcutCommand};
use crate::intent::UiIntent;
use crate::shell::NotwigUi;

use super::geometry::local_range;
use super::model::{
    ClipLane, SliceModel, SliceSelection, editor_slices, emit_slices, lower_slice_plan,
};
use super::state::ClipEditorState;
use crate::views::arranger::edit::paste_target;

pub fn handle_clip_editor_actions(
    ui: &mut NotwigUi,
    ctx: &egui::Context,
    state: &mut ClipEditorState,
    audio: &mut AudioClipData,
    clip: &notwig_core::ClipEntity,
    map: &TempoMap,
    clipboard: &Clipboard,
    playhead: TimeMus,
) {
    if ui.edit_focus != crate::state::EditFocus::ClipEditor {
        return;
    }
    if ui.shortcut_editor.capture.is_some() {
        return;
    }
    if ctx.wants_keyboard_input() {
        return;
    }
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
    let slices = editor_slices(clip, audio);
    let selected: Vec<SliceId> = state.selected.iter().copied().collect();
    let local_bar = state
        .bar_selection
        .and_then(|range| local_range(clip, range));
    let selection = if selected.is_empty() {
        local_bar.map_or_else(Selection::default, |range| {
            Selection::time(range, [ClipLane])
        })
    } else {
        Selection::objects(selected)
    };
    let model = SliceModel {
        slices: &slices,
        map: *map,
        clip_length: clip.range.length(),
        clip_start: clip.range.start,
    };

    if cut && !selection.is_empty() {
        if let Ok((extract, plan)) = edit::cut(&model, &selection) {
            apply_slice_edit(ui, state, audio, clip.range, map, &slices, plan);
            publish_clipboard(ui, ctx, extract);
        }
    } else {
        if copy
            && !selection.is_empty()
            && let Ok(extract) = edit::copy(&model, &selection)
        {
            publish_clipboard(ui, ctx, extract);
        }
        if delete
            && !selection.is_empty()
            && let Ok(plan) = edit::delete(&model, &selection)
        {
            apply_slice_edit(ui, state, audio, clip.range, map, &slices, plan);
        }
    }

    if paste {
        match clipboard {
            Clipboard::AudioClip(extract) => {
                let at = (paste_target(state.bar_selection, playhead) - clip.range.start)
                    .max(TimeMus::zero() - clip.range.start);
                if let Ok(plan) = edit::paste(&model, extract, at) {
                    apply_slice_edit(ui, state, audio, clip.range, map, &slices, plan);
                }
            }
            Clipboard::Arranger(_) => ui.emit(UiIntent::ClipboardError(
                "Clipboard belongs to another editor.".into(),
            )),
            Clipboard::Empty => {
                ui.emit(UiIntent::ClipboardError("Clipboard is empty.".into()));
            }
        }
    }

    if duplicate && !selection.is_empty() {
        if let Ok(plan) = edit::duplicate(&model, &selection) {
            apply_slice_edit(ui, state, audio, clip.range, map, &slices, plan);
        }
    }
}

fn publish_clipboard(
    ui: &mut NotwigUi,
    ctx: &egui::Context,
    extract: notwig_core::edit::Extract<ClipLane, AudioSlice>,
) {
    ctx.copy_text("notwig-clipboard".into());
    ui.emit(UiIntent::SetClipboard(Clipboard::AudioClip(extract)));
}

pub(super) fn apply_slice_edit(
    ui: &mut NotwigUi,
    state: &mut ClipEditorState,
    audio: &mut AudioClipData,
    clip_range: RangeMus,
    map: &TempoMap,
    slices: &[AudioSlice],
    plan: Plan<ClipLane, SliceId, AudioSlice>,
) {
    let lowered = lower_slice_plan(slices, map, plan);
    match &lowered.selection {
        SliceSelection::Unchanged => {}
        SliceSelection::None => {
            state.selected.clear();
            state.bar_selection = None;
        }
        SliceSelection::Objects(objects) => {
            state.selected.clear();
            state.bar_selection = None;
            for (id, _) in objects {
                state.selected.insert(*id);
            }
        }
        SliceSelection::Time(local) => {
            state.selected.clear();
            state.bar_selection =
                RangeMus::new(local.start + clip_range.start, local.end + clip_range.start).ok();
        }
    }
    emit_slices(ui, state, audio, lowered.slices);
}
