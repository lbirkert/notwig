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
    if let Some(waveform) = runtime.waveforms.get(&audio.asset_id) {
        let x1 = timeline.x_for_beat(clip.range.start, lanes.left());
        let x2 = timeline.x_for_beat(clip.range.end, lanes.left());
        if x2 > x1 {
            let full_rect =
                Rect::from_min_max(egui::pos2(x1, lanes.top()), egui::pos2(x2, lanes.bottom()));
            let gain = audio.gain_curve.as_ref().map(|curve| WaveformGain {
                curve,
                map,
                local_start: TimeMus::zero(),
            });
            draw_waveform(
                ui.painter(),
                full_rect,
                lanes,
                waveform,
                audio.source_start_sample,
                audio.source_end_sample,
                gain.as_ref(),
            );
        }
    }

    let mut local_timeline = timeline;
    local_timeline.start = timeline.start - clip.range.start;
    let mut curve = audio.gain_curve.clone().unwrap_or_default();
    let config = LineEditorConfig {
        value_min: MIN_GAIN_DB,
        value_max: MAX_GAIN_DB,
        baseline: Some(0.0),
        min_time: Some(TimeMus::zero()),
        max_time: Some(clip.range.length()),
        format_value: Some(format_gain),
    };
    let response = line_editor::show(
        ui,
        lanes,
        Id::new(("clip-gain-line", clip.id)),
        local_timeline,
        &config,
        &mut curve,
        &mut state.line_editor,
    );
    if response.committed {
        let curve = (!curve.is_empty()).then_some(curve);
        audio.gain_curve.clone_from(&curve);
        notwig.emit(UiIntent::SetClipGainCurve {
            clip_id: clip.id,
            curve,
        });
    }
}

fn format_gain(value: f64) -> String {
    format!("{value:+.2} dB")
}
