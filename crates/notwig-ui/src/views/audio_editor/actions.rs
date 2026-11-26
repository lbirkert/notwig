use std::collections::BTreeSet;

use notwig_core::edit::{self, Plan, Selection};
use notwig_core::sequencer::{AudioClipData, AudioSlice};
use notwig_core::{Curve, CurvePoint, RangeMus, SliceId, TempoMap, TimeMus};

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
    let (copy, cut, paste) = ui.shortcuts.copy_cut_paste_pressed(ctx);
    let duplicate = ui.shortcuts.was_pressed(ctx, ShortcutCommand::Duplicate);
    let delete = ui.shortcuts.was_pressed(ctx, ShortcutCommand::Delete);
    let select_all = ui.shortcuts.was_pressed(ctx, ShortcutCommand::SelectAll);
    if state.sub_editor == AudioSubEditor::Gain {
        handle_gain_actions(
            ui, ctx, state, audio, clip, clipboard, playhead, copy, cut, paste, duplicate, delete,
            select_all,
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
                "Clipboard belongs to the gain editor.".into(),
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

#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
fn handle_gain_actions(
    ui: &mut NotwigUi,
    ctx: &egui::Context,
    state: &mut ClipEditorState,
    audio: &mut AudioClipData,
    clip: &notwig_core::ClipEntity,
    clipboard: &Clipboard,
    playhead: TimeMus,
    copy: bool,
    cut: bool,
    paste: bool,
    duplicate: bool,
    delete: bool,
    select_all: bool,
) {
    let mut curve = audio.gain_curve.clone().unwrap_or_default();
    let mut changed = false;
    if select_all {
        state.curve_selection = (0..curve.len()).collect();
        state.bar_selection = None;
    }
    let selected = selected_curve_points(&curve, &state.curve_selection);
    if (copy || cut) && !selected.is_empty() {
        ctx.copy_text("notwig-clipboard".into());
        ui.emit(UiIntent::SetClipboard(Clipboard::Curve(selected.clone())));
        if cut {
            curve = remove_curve_points(&curve, &state.curve_selection);
            state.curve_selection.clear();
            state.bar_selection = None;
            changed = true;
        }
    }
    if delete && !state.curve_selection.is_empty() {
        curve = remove_curve_points(&curve, &state.curve_selection);
        state.curve_selection.clear();
        state.bar_selection = None;
        changed = true;
    }
    if duplicate && !selected.is_empty() {
        let start = selected
            .iter()
            .map(|point| point.time)
            .min()
            .unwrap_or_else(TimeMus::zero);
        let end = selected
            .iter()
            .map(|point| point.time)
            .max()
            .unwrap_or_else(TimeMus::zero);
        let mut offset = end - start;
        if offset <= TimeMus::zero() {
            offset = TimeMus::new(1, 1);
        }
        let copies: Vec<CurvePoint> = selected
            .iter()
            .map(|point| {
                let mut copy = *point;
                copy.time = copy.time + offset;
                copy
            })
            .collect();
        if let Some(selection) = append_curve_points(&mut curve, copies) {
            state.curve_selection = selection;
            state.bar_selection = None;
            changed = true;
        }
    }
    if paste {
        match clipboard {
            Clipboard::Curve(points) if !points.is_empty() => {
                let at =
                    state.bar_selection.map_or(playhead, |range| range.start) - clip.range.start;
                let origin = points
                    .iter()
                    .map(|point| point.time)
                    .min()
                    .unwrap_or_else(TimeMus::zero);
                let offset = at - origin;
                let copies: Vec<CurvePoint> = points
                    .iter()
                    .map(|point| {
                        let mut copy = *point;
                        copy.time = copy.time + offset;
                        copy
                    })
                    .collect();
                if let Some(selection) = append_curve_points(&mut curve, copies) {
                    state.curve_selection = selection;
                    state.bar_selection = None;
                    changed = true;
                }
            }
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
    if changed {
        let stored = (!curve.is_empty()).then_some(curve);
        audio.gain_curve.clone_from(&stored);
        ui.emit(UiIntent::SetClipGainCurve {
            clip_id: state.clip_id,
            curve: stored,
        });
    }
}

fn selected_curve_points(curve: &Curve, selection: &BTreeSet<usize>) -> Vec<CurvePoint> {
    selection
        .iter()
        .filter_map(|index| curve.points().get(*index))
        .copied()
        .collect()
}

fn remove_curve_points(curve: &Curve, selection: &BTreeSet<usize>) -> Curve {
    Curve::from_points(
        curve
            .points()
            .iter()
            .enumerate()
            .filter(|(index, _)| !selection.contains(index))
            .map(|(_, point)| *point),
    )
    .unwrap_or_else(|_| curve.clone())
}

fn append_curve_points(curve: &mut Curve, copies: Vec<CurvePoint>) -> Option<BTreeSet<usize>> {
    let mut tagged: Vec<(bool, CurvePoint)> = curve
        .points()
        .iter()
        .copied()
        .map(|point| (false, point))
        .collect();
    for point in copies {
        tagged.push((true, point));
    }
    tagged.sort_by_key(|(_, point)| point.time);
    let selection = tagged
        .iter()
        .enumerate()
        .filter(|(_, (added, _))| *added)
        .map(|(index, _)| index)
        .collect();
    *curve = Curve::from_points(tagged.into_iter().map(|(_, point)| point)).ok()?;
    Some(selection)
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
