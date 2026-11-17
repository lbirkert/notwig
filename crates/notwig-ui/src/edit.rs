use notwig_core::{RangeMus, TimeMus};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditTarget {
    Nodes,
    Bar,
}

impl EditTarget {
    #[must_use]
    pub fn active(nodes: usize, bar: Option<RangeMus>) -> Option<Self> {
        if nodes > 0 {
            Some(Self::Nodes)
        } else if bar.is_some_and(|range| !range.is_empty()) {
            Some(Self::Bar)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ClipboardMode {
    #[default]
    Nodes,
    Time,
}

#[must_use]
pub fn paste_target(bar: Option<RangeMus>, cursor: TimeMus) -> TimeMus {
    bar.map_or(cursor, |range| range.start).max(TimeMus::zero())
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DuplicateSpan<T> {
    pub id: T,
    pub range: RangeMus,
}

#[must_use]
pub fn node_duplicate_target<T>(spans: &[DuplicateSpan<T>]) -> Option<TimeMus> {
    spans.iter().map(|span| span.range.end).max()
}

#[must_use]
pub fn bar_duplicate_target(range: RangeMus) -> TimeMus {
    range.end
}

#[must_use]
pub fn duplicate_plan<T: Copy>(spans: &[DuplicateSpan<T>]) -> Vec<DuplicateSpan<T>> {
    let Some(base) = spans.iter().map(|span| span.range.start).min() else {
        return Vec::new();
    };
    let Some(target) = node_duplicate_target(spans) else {
        return Vec::new();
    };
    spans
        .iter()
        .map(|span| {
            let start = target + (span.range.start - base);
            let end = target + (span.range.end - base);
            DuplicateSpan {
                id: span.id,
                range: RangeMus::new(start, end).unwrap_or(span.range),
            }
        })
        .collect()
}

#[must_use]
pub fn duplicate_bar_range(range: RangeMus) -> Option<RangeMus> {
    RangeMus::new(range.end, range.end + range.length()).ok()
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

#[must_use]
pub fn clamp_bar_shift(
    origin: RangeMus,
    requested: TimeMus,
    starts: &[TimeMus],
    range_end: Option<TimeMus>,
    node_max_start: Option<TimeMus>,
) -> TimeMus {
    let range_min = TimeMus::zero() - origin.start;
    let range_max = range_end.map(|end| end - origin.end);
    let node_min = starts
        .iter()
        .copied()
        .min()
        .map_or(range_min, |start| range_min.max(TimeMus::zero() - start));
    let node_max = starts
        .iter()
        .copied()
        .max()
        .and_then(|start| node_max_start.map(|max| max - start));
    let min_shift = range_min.max(node_min);
    let max_shift = match (range_max, node_max) {
        (Some(range), Some(node)) => Some(range.min(node)),
        (Some(range), None) => Some(range),
        (None, Some(node)) => Some(node),
        (None, None) => None,
    };
    clamp_shift(requested, min_shift, max_shift)
}

fn clamp_shift(requested: TimeMus, min: TimeMus, max: Option<TimeMus>) -> TimeMus {
    match max {
        Some(max) if min <= max => requested.max(min).min(max),
        Some(_) => TimeMus::zero(),
        None => requested.max(min),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(start: i64, end: i64) -> RangeMus {
        RangeMus::new(TimeMus::new(start, 1), TimeMus::new(end, 1)).unwrap()
    }

    #[test]
    fn edit_target_prefers_nodes() {
        let bar = range(1, 3);
        assert_eq!(EditTarget::active(2, Some(bar)), Some(EditTarget::Nodes));
        assert_eq!(EditTarget::active(0, Some(bar)), Some(EditTarget::Bar));
        assert_eq!(EditTarget::active(0, None), None);
    }

    #[test]
    fn duplicate_plan_places_group_after_max_end_and_preserves_offsets() {
        let spans = [
            DuplicateSpan {
                id: 1,
                range: range(1, 3),
            },
            DuplicateSpan {
                id: 2,
                range: range(4, 5),
            },
        ];
        let plan = duplicate_plan(&spans);
        assert_eq!(plan[0].range, range(5, 7));
        assert_eq!(plan[1].range, range(8, 9));
    }

    #[test]
    fn paste_target_uses_bar_then_cursor() {
        assert_eq!(
            paste_target(Some(range(2, 4)), TimeMus::new(9, 1)),
            TimeMus::new(2, 1)
        );
        assert_eq!(paste_target(None, TimeMus::new(9, 1)), TimeMus::new(9, 1));
    }

    #[test]
    fn bar_shift_clamps_the_whole_group() {
        let origin = range(1, 2);
        let starts = [TimeMus::new(1, 1), TimeMus::new(3, 1)];
        assert_eq!(
            clamp_bar_shift(origin, TimeMus::new(-5, 1), &starts, None, None),
            TimeMus::new(-1, 1)
        );
        assert_eq!(
            clamp_bar_shift(
                origin,
                TimeMus::new(5, 1),
                &starts,
                Some(TimeMus::new(6, 1)),
                Some(TimeMus::new(5, 1))
            ),
            TimeMus::new(2, 1)
        );
    }
}
