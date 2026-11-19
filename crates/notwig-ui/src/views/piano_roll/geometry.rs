use notwig_core::{Norm01, TimeMus};

use super::state::NoteOrigin;

pub(super) const ROW_HEIGHT: f32 = 12.0;
pub(super) const TOP_PITCH: f32 = 127.0;

pub(super) fn pitch_for_y(y: f32, top: f32, scroll: f32) -> u8 {
    let row = ((y - top) / ROW_HEIGHT + scroll).floor();
    let pitch = TOP_PITCH - row;
    pitch.clamp(0.0, 127.0) as u8
}

pub(super) fn y_for_pitch(pitch: u8, top: f32, scroll: f32) -> f32 {
    let row = TOP_PITCH - f32::from(pitch) - scroll;
    top + row * ROW_HEIGHT
}

pub(super) fn clamp_pitch_scroll(scroll: f32, height: f32) -> f32 {
    let visible = (height / ROW_HEIGHT).max(1.0);
    scroll.clamp(0.0, (128.0 - visible).max(0.0))
}

pub(super) fn clamp_pitch_shift(requested: i32, pitches: &[u8]) -> i32 {
    let Some(min) = pitches.iter().copied().min() else {
        return requested;
    };
    let Some(max) = pitches.iter().copied().max() else {
        return requested;
    };
    requested.clamp(-i32::from(min), 127 - i32::from(max))
}

pub(super) fn clamp_time_shift(
    requested: TimeMus,
    origins: &[NoteOrigin],
    max_length: TimeMus,
) -> TimeMus {
    let Some(min_start) = origins.iter().map(|origin| origin.start).min() else {
        return requested;
    };
    let Some(max_end) = origins
        .iter()
        .map(|origin| origin.start + origin.length)
        .max()
    else {
        return requested;
    };
    let lower = TimeMus::zero() - min_start;
    let upper = max_length - max_end;
    if lower > upper {
        TimeMus::zero()
    } else {
        requested.max(lower).min(upper)
    }
}

pub(super) fn snapped_shift(anchor: TimeMus, delta: f64, grid: TimeMus, bypass: bool) -> TimeMus {
    crate::timeline::interaction::snapped_delta(anchor, delta, grid, bypass)
}

pub(super) fn content_time_from_beat(
    beat: TimeMus,
    clip_start: TimeMus,
    content_start: TimeMus,
) -> TimeMus {
    content_start + (beat - clip_start)
}

pub(super) fn beat_from_content_time(
    content: TimeMus,
    clip_start: TimeMus,
    content_start: TimeMus,
) -> TimeMus {
    clip_start + (content - content_start)
}

pub(super) fn velocity_for_y(y: f32, top: f32, height: f32) -> Norm01 {
    let value = 1.0 - (y - top) / height.max(1.0);
    Norm01::clamped(value.clamp(0.0, 1.0))
}

pub(super) fn velocity_y(velocity: f32, top: f32, height: f32) -> f32 {
    top + (1.0 - velocity.clamp(0.0, 1.0)) * height
}
