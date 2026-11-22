use egui::{Align2, Color32, CornerRadius, FontId, Painter, Rect, Stroke, StrokeKind};
use notwig_core::sequencer::{AudioClipData, AudioSlice, ClipKind, ClipPayload, MidiClipData};
use notwig_core::{ClipEntity, ProjectDoc, RangeMus};

use crate::theme;
use crate::timeline::TimelineGrid;
use crate::waveform::{WaveformCache, draw_waveform};

pub(crate) struct ClipVisual {
    pub clip_rect: Rect,
    pub name_rect: Rect,
}

/// Short clips must still be visible and hittable at low zoom, even when their
/// musical range maps to less than one physical pixel.
const MIN_CLIP_WIDTH: f32 = 3.0;
/// Below this width labels, note previews, and waveform previews become noise,
/// so only the clip body and selection stroke are drawn.
const MIN_CONTENT_WIDTH: f32 = 24.0;

pub(crate) fn clip_color(doc: &ProjectDoc, clip: &ClipEntity) -> Color32 {
    clip.color
        .or_else(|| doc.tracks.get(&clip.track_id).and_then(|track| track.color))
        .map_or_else(
            || match clip.kind {
                ClipKind::Midi => theme::CLIP_MIDI,
                ClipKind::Audio => theme::CLIP_AUDIO,
            },
            |color| Color32::from_rgba_unmultiplied(color.r, color.g, color.b, 255),
        )
}

fn clip_base_color(doc: &ProjectDoc, clip: &ClipEntity) -> Color32 {
    clip_color(doc, clip)
}

fn clip_label(clip: &ClipEntity) -> String {
    clip.name.clone().unwrap_or_else(|| match clip.kind {
        ClipKind::Midi => "MIDI".into(),
        ClipKind::Audio => "Audio".into(),
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_clip_visual(
    painter: &Painter,
    doc: &ProjectDoc,
    clip: &ClipEntity,
    range: RangeMus,
    lane_rect: Rect,
    grid: TimelineGrid,
    waveforms: &WaveformCache,
    selected: bool,
    preview: bool,
) -> Option<ClipVisual> {
    let x1 = grid.x_for_beat(range.start, lane_rect.left());
    let x2 = grid.x_for_beat(range.end, lane_rect.left());
    if x2 <= lane_rect.left() || x1 >= lane_rect.right() {
        return None;
    }
    let clip_rect = Rect::from_min_max(
        egui::pos2(x1, lane_rect.top() + 3.0),
        egui::pos2(x1 + (x2 - x1).max(MIN_CLIP_WIDTH), lane_rect.bottom() - 3.0),
    );
    let show_content = clip_rect.width() >= MIN_CONTENT_WIDTH;
    let color = clip_base_color(doc, clip);
    let color = if clip.muted {
        color.gamma_multiply(0.35)
    } else {
        color
    };
    let painter = painter.with_clip_rect(lane_rect);
    painter.rect_filled(clip_rect, CornerRadius::same(3), color);
    let name_height = 14.0_f32.min(clip_rect.height() * 0.42);
    let name_rect = Rect::from_min_max(
        clip_rect.min,
        egui::pos2(clip_rect.right(), clip_rect.top() + name_height),
    );
    let body_rect = Rect::from_min_max(
        egui::pos2(clip_rect.left(), name_rect.bottom()),
        clip_rect.max,
    );
    if show_content {
        painter.rect_filled(
            name_rect,
            CornerRadius {
                nw: 3,
                ne: 3,
                sw: 0,
                se: 0,
            },
            Color32::from_black_alpha(52),
        );
        match &clip.payload {
            ClipPayload::Midi(midi) => {
                draw_midi_preview(&painter, body_rect, clip.range, midi);
            }
            ClipPayload::Audio(audio) => {
                draw_audio_slices(
                    &painter,
                    doc,
                    clip,
                    audio,
                    waveforms,
                    body_rect,
                    grid,
                    lane_rect.left(),
                    lane_rect.top(),
                );
            }
        }
    }
    painter.rect_stroke(
        clip_rect,
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
    if show_content {
        let label = clip_label(clip);
        painter.with_clip_rect(name_rect).text(
            egui::pos2(name_rect.left() + 8.0, name_rect.center().y),
            Align2::LEFT_CENTER,
            label,
            FontId::monospace(9.5),
            Color32::from_black_alpha(235),
        );
    }
    if preview {
        painter.rect_stroke(
            clip_rect,
            CornerRadius::same(3),
            Stroke::new(1.5_f32, theme::ACCENT),
            StrokeKind::Inside,
        );
    }
    Some(ClipVisual {
        clip_rect,
        name_rect,
    })
}

fn draw_midi_preview(painter: &Painter, rect: Rect, range: RangeMus, midi: &MidiClipData) {
    if rect.height() < 12.0 {
        return;
    }
    let clip_start = range.start.as_f64();
    let clip_end = range.end.as_f64();
    let clip_len = range.length().as_f64().max(1e-6);
    for note in &midi.notes {
        let relative = (note.start - midi.content_start).as_f64();
        let abs_start = clip_start + relative;
        let abs_end = abs_start + note.length.as_f64();
        let start = abs_start.max(clip_start);
        let end = abs_end.min(clip_end);
        if end <= start {
            continue;
        }
        let x1 = rect.left() + ((start - clip_start) / clip_len) as f32 * rect.width();
        let x2 = rect.left() + ((end - clip_start) / clip_len) as f32 * rect.width();
        let pitch = f32::from(note.pitch);
        let y = rect.bottom() - 5.0 - (pitch / 127.0) * (rect.height() - 18.0);
        painter.rect_filled(
            Rect::from_min_max(
                egui::pos2(x1, y - 1.5),
                egui::pos2(x2.max(x1 + 1.5), y + 1.5),
            ),
            CornerRadius::same(1),
            Color32::from_black_alpha(150),
        );
    }
}

pub(crate) fn audio_slices(clip: &ClipEntity, audio: &AudioClipData) -> Vec<AudioSlice> {
    if audio.uses_slices() {
        return audio.slices.clone();
    }
    vec![AudioSlice {
        id: notwig_core::SliceId::from_uuid(*clip.id.as_uuid()),
        start: notwig_core::TimeMus::zero(),
        source_start_sample: audio.source_start_sample,
        source_end_sample: audio.source_end_sample,
        silent_prefix_samples: 0,
        gain_db: audio.gain_db,
        asset_id: None,
    }]
}

pub(crate) fn draw_audio_slices(
    painter: &Painter,
    doc: &ProjectDoc,
    clip: &ClipEntity,
    audio: &AudioClipData,
    waveforms: &WaveformCache,
    body_rect: Rect,
    grid: TimelineGrid,
    lane_left: f32,
    lane_top: f32,
) {
    if !audio.uses_slices() {
        if let Some(waveform) = waveforms.get(&audio.asset_id) {
            draw_waveform(
                painter,
                body_rect,
                body_rect,
                waveform,
                audio.source_start_sample,
                audio.source_end_sample,
            );
        }
        return;
    }
    let map = doc.tempo_map();
    for slice in &audio.slices {
        let start = clip.range.start + slice.start;
        let duration = map.samples_to_mus(slice.duration_samples());
        let x1 = grid.x_for_beat(start, lane_left);
        let x2 = grid.x_for_beat(start + duration, lane_left);
        let full_rect = Rect::from_min_max(
            egui::pos2(x1, body_rect.top()),
            egui::pos2(x2, body_rect.bottom()),
        );
        let clip_rect = Rect::from_min_max(egui::pos2(body_rect.left(), lane_top), body_rect.max);
        let rect = full_rect.intersect(clip_rect);
        let asset_id = slice.asset_id.unwrap_or(audio.asset_id);
        let Some(waveform) = waveforms.get(&asset_id) else {
            continue;
        };
        if rect.width() > 1.0 {
            let audio_start = start + map.samples_to_mus(slice.silent_prefix_samples);
            let audio_duration = map.samples_to_mus(slice.audio_samples());
            let audio_rect = Rect::from_min_max(
                egui::pos2(grid.x_for_beat(audio_start, lane_left), body_rect.top()),
                egui::pos2(
                    grid.x_for_beat(audio_start + audio_duration, lane_left),
                    body_rect.bottom(),
                ),
            );
            draw_waveform(
                painter,
                audio_rect,
                clip_rect,
                waveform,
                slice.source_start_sample,
                slice.source_end_sample,
            );
        }
    }
}

#[doc(hidden)]
pub mod test_api {
    #[allow(clippy::wildcard_imports)]
    use super::*;

    pub fn clip_label(clip: &ClipEntity) -> String {
        super::clip_label(clip)
    }

    pub fn clip_base_color(doc: &ProjectDoc, clip: &ClipEntity) -> Color32 {
        super::clip_base_color(doc, clip)
    }

    pub fn clip_color(doc: &ProjectDoc, clip: &ClipEntity) -> Color32 {
        super::clip_color(doc, clip)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn clip_visual_rects(
        painter: &Painter,
        doc: &ProjectDoc,
        clip: &ClipEntity,
        range: RangeMus,
        lane_rect: Rect,
        grid: TimelineGrid,
        waveforms: &WaveformCache,
        selected: bool,
        preview: bool,
    ) -> Option<(Rect, Rect)> {
        super::draw_clip_visual(
            painter, doc, clip, range, lane_rect, grid, waveforms, selected, preview,
        )
        .map(|visual| (visual.clip_rect, visual.name_rect))
    }
}
