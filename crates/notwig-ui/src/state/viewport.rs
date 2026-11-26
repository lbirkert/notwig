use notwig_core::TimeMus;

use crate::widgets::beats_to_time;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub start: TimeMus,
    pub pixels_per_beat: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            start: TimeMus::zero(),
            pixels_per_beat: 64.0,
        }
    }
}

impl Viewport {
    pub const MIN_PPB: f32 = 4.0;
    pub const MAX_PPB: f32 = 1_000_000.0;

    #[must_use]
    pub fn beat_to_x(self, beat: TimeMus, timeline_left: f32) -> f32 {
        timeline_left + ((beat.as_f64() - self.start.as_f64()) as f32) * self.pixels_per_beat
    }

    #[must_use]
    pub fn x_to_beat(self, x: f32, timeline_left: f32) -> TimeMus {
        // `max` also rejects NaN (f32::max returns the non-NaN operand), so a
        // degenerate scale cannot produce an infinite beat. Quantize to
        // micro-beats so repeated conversions stay on the same rational grid.
        let beats = f64::from(x - timeline_left) / f64::from(self.pixels_per_beat.max(0.001))
            + self.start.as_f64();
        beats_to_time(beats.max(0.0))
    }

    #[must_use]
    pub fn visible_beats(self, width: f32) -> f64 {
        f64::from((width / self.pixels_per_beat.max(0.001)).max(1.0))
    }

    pub fn zoom_by(&mut self, factor: f32) {
        self.pixels_per_beat = (self.pixels_per_beat * factor).clamp(Self::MIN_PPB, Self::MAX_PPB);
    }

    /// Zooms by `factor` while keeping the beat under `pointer_x` fixed.
    ///
    /// The viewport start is re-derived from the clamped new scale so the beat
    /// under the pointer is invariant across the step; `start` never goes
    /// negative.
    pub fn zoom_at(&mut self, pointer_x: f32, timeline_left: f32, factor: f32) {
        let ppb = self.pixels_per_beat.clamp(Self::MIN_PPB, Self::MAX_PPB);
        let beat_under = self.start.as_f64() + f64::from((pointer_x - timeline_left) / ppb);
        let new_ppb = (ppb * factor).clamp(Self::MIN_PPB, Self::MAX_PPB);
        let new_start = (beat_under - f64::from((pointer_x - timeline_left) / new_ppb)).max(0.0);
        self.pixels_per_beat = new_ppb;
        self.start = beats_to_time(new_start);
    }

    /// Zooms without clamping the viewport start at zero.
    ///
    /// Clip editors use this so their timeline can pan left of the zeroth bar.
    pub fn zoom_at_unbounded(&mut self, pointer_x: f32, timeline_left: f32, factor: f32) {
        let ppb = self.pixels_per_beat.clamp(Self::MIN_PPB, Self::MAX_PPB);
        let beat_under = self.start.as_f64() + f64::from((pointer_x - timeline_left) / ppb);
        let new_ppb = (ppb * factor).clamp(Self::MIN_PPB, Self::MAX_PPB);
        let new_start = beat_under - f64::from((pointer_x - timeline_left) / new_ppb);
        self.pixels_per_beat = new_ppb;
        self.start = beats_to_time(new_start);
    }

    pub fn pan_beats(&mut self, delta: f64) {
        let start = (self.start.as_f64() + delta).max(0.0);
        self.start = beats_to_time(start);
    }

    /// Pans without clamping the viewport start at zero.
    pub fn pan_beats_unbounded(&mut self, delta: f64) {
        self.start = beats_to_time(self.start.as_f64() + delta);
    }

    /// Converts an x coordinate without clamping the resulting beat at zero.
    #[must_use]
    pub fn x_to_beat_unbounded(self, x: f32, timeline_left: f32) -> TimeMus {
        let beats = f64::from(x - timeline_left) / f64::from(self.pixels_per_beat.max(0.001))
            + self.start.as_f64();
        beats_to_time(beats)
    }

    #[must_use]
    pub fn scroll_start(self, content_end: f64, visible_beats: f64) -> f64 {
        self.start
            .as_f64()
            .clamp(0.0, (content_end - visible_beats).max(0.0))
    }

    #[must_use]
    pub fn scroll_thumb(
        self,
        content_end: f64,
        visible_beats: f64,
        track_width: f32,
    ) -> (f32, f32) {
        let content = content_end.max(visible_beats).max(1.0);
        let start = self.scroll_start(content_end, visible_beats);
        let thumb_x = (start / content) as f32 * track_width;
        let thumb_w = ((visible_beats / content) as f32 * track_width)
            .max(24.0)
            .min(track_width);
        (thumb_x, thumb_w)
    }
}
