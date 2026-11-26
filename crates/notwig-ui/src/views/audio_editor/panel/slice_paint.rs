use egui::{Align2, Color32, CornerRadius, FontId, Rect, Stroke, StrokeKind, Vec2};
use notwig_core::sequencer::{AudioClipData, AudioSlice};
use notwig_core::{ClipEntity, ProjectDoc, TempoMap, TimeMus};

use crate::runtime::RuntimeView;
use crate::theme;
use crate::timeline::TimelineGrid;
use crate::waveform::WaveformGain;

use super::super::geometry::SliceGeometry;
use super::event_color;

pub(super) struct SlicePaint {
    pub(super) slice_rect: Rect,
    pub(super) title_rect: Rect,
    pub(super) body_rect: Rect,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_slice(
    painter: &egui::Painter,
    doc: &ProjectDoc,
    runtime: &RuntimeView,
    clip: &ClipEntity,
    audio: &AudioClipData,
    slice: &AudioSlice,
    geometry: &SliceGeometry,
    start: TimeMus,
    timeline_grid: TimelineGrid,
    content_left: f32,
    clip_start: TimeMus,
    map: &TempoMap,
    slice_top: f32,
    slice_bottom: f32,
    lanes: Rect,
    selected: bool,
) -> SlicePaint {
    let source_start = geometry.source_start;
    let source_end = geometry.source_end;
    let duration = geometry.duration(map);
    let x1 = timeline_grid.x_for_beat(clip_start + start, content_left);
    let x2 = timeline_grid.x_for_beat(clip_start + start + duration, content_left);
    let full_rect = Rect::from_min_max(
        egui::pos2(x1, slice_top),
        egui::pos2(x2.max(x1 + 2.0), slice_bottom),
    );
    let slice_rect = full_rect.intersect(lanes);
    let color = event_color(doc, clip);
    let color = if clip.muted {
        color.gamma_multiply(0.35)
    } else {
        color
    };
    painter.rect_filled(slice_rect, CornerRadius::same(3), color);
    let asset_id = slice.asset_id.unwrap_or(audio.asset_id);
    if let Some(waveform) = runtime.waveforms.get(&asset_id) {
        let audio_start = start + map.samples_to_mus(geometry.silent_prefix);
        let audio_duration = map.samples_to_mus(source_end.saturating_sub(source_start));
        let audio_x1 = timeline_grid.x_for_beat(clip_start + audio_start, content_left);
        let audio_x2 =
            timeline_grid.x_for_beat(clip_start + audio_start + audio_duration, content_left);
        let audio_rect = Rect::from_min_max(
            egui::pos2(audio_x1, slice_top),
            egui::pos2(audio_x2.max(audio_x1 + 1.0), slice_bottom),
        );
        let local_start = start + map.samples_to_mus(geometry.silent_prefix);
        let gain = audio.gain_curve.as_ref().map(|curve| WaveformGain {
            curve,
            map: *map,
            local_start,
        });
        crate::waveform::draw_waveform(
            painter,
            audio_rect.shrink2(Vec2::new(2.0, 4.0)),
            lanes,
            waveform,
            source_start,
            source_end,
            gain.as_ref(),
        );
    }
    painter.rect_stroke(
        slice_rect,
        CornerRadius::same(3),
        Stroke::new(
            if selected { 1.8_f32 } else { 1.0_f32 },
            if selected {
                theme::SELECTION
            } else {
                Color32::from_black_alpha(120)
            },
        ),
        StrokeKind::Inside,
    );
    let name_height = 14.0_f32.min(slice_rect.height() * 0.42);
    let title_rect = Rect::from_min_max(
        slice_rect.min,
        egui::pos2(slice_rect.right(), slice_rect.top() + name_height),
    );
    painter.rect_filled(
        title_rect,
        CornerRadius {
            nw: 3,
            ne: 3,
            sw: 0,
            se: 0,
        },
        Color32::from_black_alpha(52),
    );
    let label = doc.assets.assets.get(&asset_id).map_or("event", |asset| {
        asset
            .original_name
            .as_deref()
            .unwrap_or_else(|| asset.uri.rsplit('/').next().unwrap_or("event"))
    });
    painter.with_clip_rect(title_rect).text(
        egui::pos2(title_rect.left() + 8.0, title_rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::monospace(9.5),
        Color32::from_black_alpha(235),
    );
    let body_rect = Rect::from_min_max(
        egui::pos2(slice_rect.left(), title_rect.bottom()),
        slice_rect.max,
    );
    SlicePaint {
        slice_rect,
        title_rect,
        body_rect,
    }
}
