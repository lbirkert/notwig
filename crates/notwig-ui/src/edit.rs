use notwig_core::{RangeMus, TimeMus};

#[must_use]
pub fn paste_target(bar: Option<RangeMus>, cursor: TimeMus) -> TimeMus {
    bar.map_or(cursor, |range| range.start).max(TimeMus::zero())
}

#[must_use]
pub fn clamp_node_shift(
    requested: TimeMus,
    starts: &[TimeMus],
    max_start: Option<TimeMus>,
) -> TimeMus {
    let Some(min_start) = starts.iter().copied().min() else {
        return requested;
    };
    let max_node_start = starts.iter().copied().max().unwrap_or(min_start);
    let min_shift = TimeMus::zero() - min_start;
    let max_shift = max_start.map(|max| max - max_node_start);
    clamp_shift(requested, min_shift, max_shift)
}

fn clamp_shift(requested: TimeMus, min: TimeMus, max: Option<TimeMus>) -> TimeMus {
    match max {
        Some(max) if min <= max => requested.max(min).min(max),
        Some(_) => TimeMus::zero(),
        None => requested.max(min),
    }
}
