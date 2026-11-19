use egui::{Painter, Rect, Stroke};
use notwig_core::TimeMus;

use crate::theme;
use crate::widgets::{snap_step, snap_time};

#[derive(Debug, Clone, Copy)]
pub struct TimelineGrid {
    pub start: TimeMus,
    pub pixels_per_beat: f32,
    pub beats_per_bar: u8,
    pub step: TimeMus,
}

impl TimelineGrid {
    #[must_use]
    pub fn new(start: TimeMus, pixels_per_beat: f32, beats_per_bar: u8) -> Self {
        Self {
            start,
            pixels_per_beat,
            beats_per_bar: beats_per_bar.max(1),
            step: snap_step(pixels_per_beat, beats_per_bar),
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
        snap_time(beat, self.step, bypass)
    }

    pub fn draw(self, painter: &Painter, rect: Rect) {
        let visible_beats = f64::from((rect.width() / self.pixels_per_beat).max(1.0));
        let step = self.step.as_f64().max(1.0 / 256.0);
        let bar_len = f64::from(self.beats_per_bar);
        let first = (self.start.as_f64() / step).floor() as i64;
        let last = ((self.start.as_f64() + visible_beats) / step).ceil() as i64 + 1;
        for index in first..=last {
            let beat = index as f64 * step;
            if beat < 0.0 {
                continue;
            }
            let x = self.x_for_beat(
                TimeMus::new((beat * 1_000_000.0) as i64, 1_000_000),
                rect.left(),
            );
            if x < rect.left() - 1.0 || x > rect.right() + 1.0 {
                continue;
            }
            let strong = (beat / bar_len).fract().abs() < 1e-6;
            let beat_line = beat.fract().abs() < 1e-6;
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                Stroke::new(
                    1.0_f32,
                    if strong {
                        egui::Color32::from_rgb(92, 97, 112)
                    } else if beat_line {
                        theme::GRID_STRONG
                    } else {
                        theme::GRID
                    },
                ),
            );
        }
    }
}
