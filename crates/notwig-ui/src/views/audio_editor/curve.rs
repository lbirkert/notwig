//! Shared clip curve editor.
//!
//! Built-in Gain and extension-supplied curve sub-editors use the same
//! `widgets::line_editor` surface and canonical clip curve command path.

use egui::{Id, Rect};
use notwig_core::sequencer::AudioClipData;
use notwig_core::{ClipEntity, Curve, TempoMap, TimeMus};
use notwig_ext_api::CurveTarget;

use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::shell::NotwigUi;
use crate::timeline::TimelineGrid;
use crate::waveform::{WaveformGain, draw_waveform};
use crate::widgets::line_editor::{self, LineEditorConfig};

use super::state::ClipEditorState;

/// Lowest gain offset shown by the gain sub-editor.
pub(super) const MIN_GAIN_DB: f64 = -60.0;
/// Highest gain offset shown by the gain sub-editor.
pub(super) const MAX_GAIN_DB: f64 = 24.0;

/// Layout and formatting for one shared curve sub-editor.
#[derive(Debug, Clone, Copy)]
pub(super) struct CurveEditorStyle {
    value_min: f64,
    value_max: f64,
    baseline: Option<f64>,
    format: fn(f64) -> String,
}

impl CurveEditorStyle {
    fn config(self) -> LineEditorConfig {
        LineEditorConfig {
            value_min: self.value_min,
            value_max: self.value_max,
            baseline: self.baseline,
            min_time: None,
            max_time: None,
            format_value: Some(self.format),
        }
    }
}

/// Returns the built-in gain curve style.
pub(super) fn gain_style() -> CurveEditorStyle {
    CurveEditorStyle {
        value_min: MIN_GAIN_DB,
        value_max: MAX_GAIN_DB,
        baseline: Some(0.0),
        format: format_gain,
    }
}

/// Returns the canonical curve for `target`.
pub(super) fn target_curve(audio: &AudioClipData, target: CurveTarget) -> Option<&Curve> {
    match target {
        CurveTarget::Gain => audio.gain_curve.as_ref(),
        CurveTarget::Pan => audio.pan_curve.as_ref(),
    }
}

pub(super) fn target_curve_mut(
    audio: &mut AudioClipData,
    target: CurveTarget,
) -> &mut Option<Curve> {
    match target {
        CurveTarget::Gain => &mut audio.gain_curve,
        CurveTarget::Pan => &mut audio.pan_curve,
    }
}

/// Draws one shared curve sub-editor and commits edits as a host intent.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_curve_sub_editor(
    notwig: &mut NotwigUi,
    ui: &mut egui::Ui,
    state: &mut ClipEditorState,
    audio: &mut AudioClipData,
    clip: &ClipEntity,
    runtime: &RuntimeView,
    map: TempoMap,
    timeline: TimelineGrid,
    lanes: Rect,
    target: CurveTarget,
    style: CurveEditorStyle,
) {
    let mut local_timeline = timeline;
    local_timeline.start = timeline.start - clip.range.start;
    let mut curve = target_curve(audio, target).cloned().unwrap_or_default();
    let mut local_time_selection = state.bar_selection.and_then(|range| {
        notwig_core::RangeMus::new(range.start - clip.range.start, range.end - clip.range.start)
            .ok()
    });
    let response = line_editor::show_with_background(
        ui,
        lanes,
        Id::new(("clip-curve-line", clip.id, target.as_str())),
        local_timeline,
        &style.config(),
        &mut curve,
        &mut state.curve_selection,
        &mut local_time_selection,
        &mut state.line_editor,
        &mut |ui, curve| {
            let background = if target == CurveTarget::Gain {
                Some(curve)
            } else {
                target_curve(audio, CurveTarget::Gain)
            };
            draw_clip_waveform(ui, runtime, audio, clip, map, timeline, lanes, background);
        },
    );
    if let Some(selected) = selected_curve_range(&curve, &state.curve_selection) {
        let start =
            local_time_selection.map_or(selected.start, |range| range.start.min(selected.start));
        let end = local_time_selection.map_or(selected.end, |range| range.end.max(selected.end));
        local_time_selection = notwig_core::RangeMus::new(start, end).ok();
    }
    state.bar_selection = local_time_selection.and_then(|range| {
        notwig_core::RangeMus::new(range.start + clip.range.start, range.end + clip.range.start)
            .ok()
    });
    if response.committed {
        let stored = (!curve.is_empty()).then_some(curve);
        target_curve_mut(audio, target).clone_from(&stored);
        emit_curve_intent(notwig, target, clip.id, stored, false);
    } else if response.changed {
        emit_curve_intent(
            notwig,
            target,
            clip.id,
            (!curve.is_empty()).then_some(curve),
            true,
        );
    }
}

/// Emits the commit or preview intent for a canonical curve target.
pub(super) fn emit_curve_intent(
    notwig: &mut NotwigUi,
    target: CurveTarget,
    clip_id: notwig_core::ClipId,
    curve: Option<Curve>,
    preview: bool,
) {
    let intent = match (target, preview) {
        (CurveTarget::Gain, false) => UiIntent::SetClipGainCurve { clip_id, curve },
        (CurveTarget::Gain, true) => UiIntent::PreviewClipGainCurve { clip_id, curve },
        (CurveTarget::Pan, false) => UiIntent::SetClipPanCurve { clip_id, curve },
        (CurveTarget::Pan, true) => UiIntent::PreviewClipPanCurve { clip_id, curve },
    };
    notwig.emit(intent);
}

#[allow(clippy::too_many_arguments)]
fn draw_clip_waveform(
    ui: &mut egui::Ui,
    runtime: &RuntimeView,
    audio: &AudioClipData,
    clip: &ClipEntity,
    map: TempoMap,
    timeline: TimelineGrid,
    lanes: Rect,
    curve: Option<&Curve>,
) {
    if audio.uses_slices() {
        for slice in &audio.slices {
            let asset_id = slice.asset_id.unwrap_or(audio.asset_id);
            let Some(waveform) = runtime.waveforms.get(&asset_id) else {
                continue;
            };
            let silent = map.samples_to_mus(slice.silent_prefix_samples);
            draw_gain_waveform(
                ui,
                waveform,
                clip.range.start + slice.start + silent,
                map.samples_to_mus(slice.audio_samples()),
                slice.source_start_sample,
                slice.source_end_sample,
                curve,
                map,
                slice.start + silent,
                timeline,
                lanes,
            );
        }
        return;
    }
    let Some(waveform) = runtime.waveforms.get(&audio.asset_id) else {
        return;
    };
    draw_gain_waveform(
        ui,
        waveform,
        clip.range.start,
        clip.range.length(),
        audio.source_start_sample,
        audio.source_end_sample,
        curve,
        map,
        TimeMus::zero(),
        timeline,
        lanes,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_gain_waveform(
    ui: &mut egui::Ui,
    waveform: &crate::waveform::AudioWaveform,
    start: TimeMus,
    duration: TimeMus,
    source_start: u64,
    source_end: u64,
    curve: Option<&Curve>,
    map: TempoMap,
    local_start: TimeMus,
    timeline: TimelineGrid,
    lanes: Rect,
) {
    if duration <= TimeMus::zero() {
        return;
    }
    let x1 = timeline.x_for_beat(start, lanes.left());
    let x2 = timeline.x_for_beat(start + duration, lanes.left());
    if x2 <= x1 {
        return;
    }
    let full_rect = Rect::from_min_max(egui::pos2(x1, lanes.top()), egui::pos2(x2, lanes.bottom()));
    let gain = curve.map(|curve| WaveformGain {
        curve,
        map,
        local_start,
    });
    draw_waveform(
        ui.painter(),
        full_rect,
        lanes,
        waveform,
        source_start,
        source_end,
        gain.as_ref(),
    );
}

fn selected_curve_range(
    curve: &Curve,
    selection: &std::collections::BTreeSet<usize>,
) -> Option<notwig_core::RangeMus> {
    let mut start = None;
    let mut end = None;
    for index in selection {
        let Some(point) = curve.points().get(*index) else {
            continue;
        };
        start = Some(start.map_or(point.time, |value: TimeMus| value.min(point.time)));
        end = Some(end.map_or(point.time, |value: TimeMus| value.max(point.time)));
    }
    notwig_core::RangeMus::new(start?, end?).ok()
}

fn format_gain(value: f64) -> String {
    format!("{value:+.2} dB")
}
