use notwig_core::edit::{self, Plan, Selection};
use notwig_core::sequencer::{AudioClipData, AudioSlice};
use notwig_core::{RangeMus, SliceId, TempoMap, TimeMus};
use notwig_ext_api::CurveTarget;

use crate::Clipboard;
use crate::commands::ShortcutCommand;
use crate::intent::UiIntent;
use crate::shell::NotwigUi;

use super::geometry::local_range;
use super::model::{
    ClipLane, SliceModel, SliceSelection, editor_slices, emit_slices, lower_slice_plan,
};
use super::state::{AudioSubEditor, ClipEditorState};
use crate::views::arranger::edit::paste_target;
use crate::widgets::curve_selection::{CurveEditActions, apply_curve_edit_actions};

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
    if state.sub_editor.extension_id().is_some() {
        return;
    }
    let (copy, cut, paste) = ui.shortcuts.copy_cut_paste_pressed(ctx);
    let duplicate = ui.shortcuts.was_pressed(ctx, ShortcutCommand::Duplicate);
    let delete = ui.shortcuts.was_pressed(ctx, ShortcutCommand::Delete);
    let select_all = ui.shortcuts.was_pressed(ctx, ShortcutCommand::SelectAll);
    if let Some(target) = active_curve_target(state) {
        handle_curve_actions(
            ui, ctx, state, audio, clip, clipboard, playhead, target, copy, cut, paste, duplicate,
            delete, select_all,
        );
        return;
    }
    let slices = editor_slices(clip, audio);
    if select_all {
        state.select_all(slices.iter().map(|slice| slice.id));
    }
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
            Clipboard::Devices => ui.emit(UiIntent::ClipboardError(
                "Clipboard contains devices.".into(),
            )),
            Clipboard::Curve(_) => ui.emit(UiIntent::ClipboardError(
                "Clipboard belongs to a curve editor.".into(),
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

fn active_curve_target(state: &ClipEditorState) -> Option<CurveTarget> {
    match &state.sub_editor {
        AudioSubEditor::Gain => Some(CurveTarget::Gain),
        AudioSubEditor::Extension(_) | AudioSubEditor::AudioEvents => None,
    }
}

#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
fn handle_curve_actions(
    ui: &mut NotwigUi,
    ctx: &egui::Context,
    state: &mut ClipEditorState,
    audio: &mut AudioClipData,
    clip: &notwig_core::ClipEntity,
    clipboard: &Clipboard,
    playhead: TimeMus,
    target: CurveTarget,
    copy: bool,
    cut: bool,
    paste: bool,
    duplicate: bool,
    delete: bool,
    select_all: bool,
) {
    let mut curve = super::curve::target_curve(audio, target)
        .cloned()
        .unwrap_or_default();
    let paste_at = state.bar_selection.map_or(playhead, |range| range.start) - clip.range.start;
    let actions = CurveEditActions {
        copy,
        cut,
        paste,
        duplicate,
        delete,
        select_all,
        paste_at,
    };
    let clipboard_points = match clipboard {
        Clipboard::Curve(points) => points.as_slice(),
        _ => &[],
    };
    let result = apply_curve_edit_actions(
        &mut curve,
        &mut state.curve_selection,
        &mut state.bar_selection,
        &actions,
        clipboard_points,
    );
    if let Some(points) = result.copied {
        ctx.copy_text("notwig-clipboard".into());
        ui.emit(UiIntent::SetClipboard(Clipboard::Curve(points)));
    }
    if paste {
        match clipboard {
            Clipboard::Curve(points) if !points.is_empty() => {}
            Clipboard::Curve(_) | Clipboard::Empty => {
                ui.emit(UiIntent::ClipboardError("Clipboard is empty.".into()));
            }
            Clipboard::Arranger(_) => ui.emit(UiIntent::ClipboardError(
                "Clipboard belongs to another editor.".into(),
            )),
            Clipboard::Devices => ui.emit(UiIntent::ClipboardError(
                "Clipboard contains devices.".into(),
            )),
            Clipboard::AudioClip(_) => ui.emit(UiIntent::ClipboardError(
                "Clipboard belongs to the audio events editor.".into(),
            )),
        }
    }
    if result.changed {
        let stored = (!curve.is_empty()).then_some(curve);
        super::curve::target_curve_mut(audio, target).clone_from(&stored);
        super::curve::emit_curve_intent(ui, target, state.clip_id, stored, false);
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
