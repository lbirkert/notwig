//! Clip gain line editor.

use egui::{Id, Rect};
use notwig_core::sequencer::AudioClipData;
use notwig_core::{ClipEntity, TempoMap, TimeMus};

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

/// Draws the clip gain line editor and commits edits as one intent.
pub(super) fn draw_gain_line_editor(
    notwig: &mut NotwigUi,
    ui: &mut egui::Ui,
    state: &mut ClipEditorState,
    audio: &mut AudioClipData,
    clip: &ClipEntity,
    runtime: &RuntimeView,
    map: TempoMap,
    timeline: TimelineGrid,
    lanes: Rect,
) {
    let mut local_timeline = timeline;
    local_timeline.start = timeline.start - clip.range.start;
    let mut curve = audio.gain_curve.clone().unwrap_or_default();
    let mut local_time_selection = state.bar_selection.and_then(|range| {
        notwig_core::RangeMus::new(range.start - clip.range.start, range.end - clip.range.start)
            .ok()
    });
    let config = LineEditorConfig {
        value_min: MIN_GAIN_DB,
        value_max: MAX_GAIN_DB,
        baseline: Some(0.0),
        min_time: Some(TimeMus::zero()),
        max_time: Some(clip.range.length()),
        format_value: Some(format_gain),
    };
    let response = line_editor::show_with_background(
        ui,
        lanes,
        Id::new(("clip-gain-line", clip.id)),
        local_timeline,
        &config,
        &mut curve,
        &mut state.curve_selection,
        &mut local_time_selection,
        &mut state.line_editor,
        &mut |ui, curve| {
            draw_clip_waveform(
                ui,
                runtime,
                audio,
                clip,
                map,
                timeline,
                lanes,
                (!curve.is_empty()).then_some(curve),
            );
        },
    );
    state.bar_selection = local_time_selection.and_then(|range| {
        notwig_core::RangeMus::new(range.start + clip.range.start, range.end + clip.range.start)
            .ok()
    });
    if response.committed {
        let curve = (!curve.is_empty()).then_some(curve);
        audio.gain_curve.clone_from(&curve);
        notwig.emit(UiIntent::SetClipGainCurve {
            clip_id: clip.id,
            curve,
        });
    } else if response.changed {
        notwig.emit(UiIntent::PreviewClipGainCurve {
            clip_id: clip.id,
            curve: (!curve.is_empty()).then_some(curve),
        });
    }
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
    curve: Option<&notwig_core::Curve>,
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
    curve: Option<&notwig_core::Curve>,
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

fn format_gain(value: f64) -> String {
    format!("{value:+.2} dB")
}
