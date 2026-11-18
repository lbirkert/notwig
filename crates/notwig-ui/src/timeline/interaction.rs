use egui::Pos2;
use notwig_core::{RangeMus, TimeMus};

pub(crate) fn pointer_delta(start: Pos2, current: Pos2, ppb: f32) -> f64 {
    if ppb <= 0.0 {
        return 0.0;
    }
    f64::from(current.x - start.x) / f64::from(ppb)
}

pub(crate) fn snapped_delta(anchor: TimeMus, delta: f64, grid: TimeMus, bypass: bool) -> TimeMus {
    let beat = TimeMus::new(
        ((anchor.as_f64() + delta) * 1_000_000.0).round() as i64,
        1_000_000,
    );
    crate::widgets::snap_time(beat, grid, bypass) - anchor
}

pub(crate) fn selection_range(anchor: TimeMus, current: TimeMus) -> Option<RangeMus> {
    let (start, end) = if anchor <= current {
        (anchor, current)
    } else {
        (current, anchor)
    };
    RangeMus::new(start, end)
        .ok()
        .filter(|range| !range.is_empty())
}
