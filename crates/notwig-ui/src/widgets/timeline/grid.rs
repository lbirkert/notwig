use egui::{Painter, Rect, Stroke};
use notwig_core::TimeMus;

use crate::theme;
use crate::widgets::{SnapSettings, snap_step_label};

#[derive(Debug, Clone, Copy)]
pub struct TimelineGrid {
    pub start: TimeMus,
    pub pixels_per_beat: f32,
    pub bar_length: TimeMus,
    pub beat_length: TimeMus,
    /// The one effective step used to paint the grid, snap gestures, and
    /// labels.
    pub step: TimeMus,
    pub snap_enabled: bool,
}

impl TimelineGrid {
    #[must_use]
    pub fn new(
        start: TimeMus,
        pixels_per_beat: f32,
        bar_length: TimeMus,
        beat_length: TimeMus,
        snap: SnapSettings,
    ) -> Self {
        let bar_length = positive_length(bar_length, TimeMus::new(4, 1));
        let beat_length = positive_length(beat_length, TimeMus::new(1, 1));
        Self {
            start,
            pixels_per_beat,
            bar_length,
            beat_length,
            step: snap.step(pixels_per_beat, bar_length),
            snap_enabled: snap.enabled,
        }
    }

    #[must_use]
    pub fn x_for_beat(self, beat: TimeMus, left: f32) -> f32 {
        left + (beat.as_f64() - self.start.as_f64()) as f32 * self.pixels_per_beat
    }

    #[must_use]
    pub fn beat_for_x(self, x: f32, left: f32) -> TimeMus {
        let beat = self.start.as_f64() + f64::from((x - left) / self.pixels_per_beat);
        TimeMus::new((beat * 1_000_000.0).round() as i64, 1_000_000)
    }

    #[must_use]
    pub fn snap(self, beat: TimeMus, bypass: bool) -> TimeMus {
        crate::widgets::snap_time(beat, self.step, !self.snap_enabled || bypass)
    }

    #[must_use]
    pub fn bypassed(self, modifiers: egui::Modifiers) -> bool {
        !self.snap_enabled || modifiers.shift || modifiers.alt
    }

    #[must_use]
    pub fn step_label(self) -> String {
        snap_step_label(self.step, self.bar_length)
    }

    pub fn draw(self, painter: &Painter, rect: Rect) {
        let visible_beats = f64::from((rect.width() / self.pixels_per_beat).max(1.0));
        let start = self.start.as_f64();
        let end = start + visible_beats;
        let bar = self.bar_length.as_f64().max(1e-9);
        let beat_length = self.beat_length.as_f64().max(1e-9);
        let step = self.step.as_f64();

        if step > 0.0 && step as f32 * self.pixels_per_beat >= 1.0 {
            for index in line_indices(start, end, step) {
                let beat = index as f64 * step;
                if beat < 0.0 || aligned(beat, bar) || aligned(beat, beat_length) {
                    continue;
                }
                self.draw_line(painter, rect, beat, theme::GRID);
            }
        }

        for index in line_indices(start, end, beat_length) {
            let beat = index as f64 * beat_length;
            if beat < 0.0 || aligned(beat, bar) {
                continue;
            }
            self.draw_line(painter, rect, beat, theme::GRID_STRONG);
        }

        for index in line_indices(start, end, bar) {
            let beat = index as f64 * bar;
            if beat < 0.0 {
                continue;
            }
            self.draw_line(painter, rect, beat, egui::Color32::from_rgb(92, 97, 112));
        }
    }

    fn draw_line(&self, painter: &Painter, rect: Rect, beat: f64, color: egui::Color32) {
        let x = self.x_for_beat(
            TimeMus::new((beat * 1_000_000.0).round() as i64, 1_000_000),
            rect.left(),
        );
        if x < rect.left() - 1.0 || x > rect.right() + 1.0 {
            return;
        }
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            Stroke::new(1.0_f32, grid_color(color, self.snap_enabled)),
        );
    }
}

pub(crate) fn line_indices(start: f64, end: f64, step: f64) -> std::ops::RangeInclusive<i64> {
    let step = step.max(1e-9);
    let first = (start / step).floor() as i64;
    let last = (end / step).ceil() as i64 + 1;
    first..=last
}

pub(crate) fn aligned(beat: f64, step: f64) -> bool {
    if step <= 0.0 {
        return true;
    }
    let ratio = beat / step;
    (ratio - ratio.round()).abs() < 1e-6
}

fn positive_length(value: TimeMus, fallback: TimeMus) -> TimeMus {
    if value > TimeMus::zero() {
        value
    } else {
        fallback
    }
}

pub(crate) fn grid_color(color: egui::Color32, snap_enabled: bool) -> egui::Color32 {
    if snap_enabled {
        color
    } else {
        color.gamma_multiply(0.55)
    }
}
