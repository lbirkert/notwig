use std::collections::BTreeSet;

use crate::edit::{
    EditError, EditModel, EditOp, EditPlan, NextSelection, ObjectRef, Plan, Selection,
    SelectionMode, TimeSelection, TrimLimits, epsilon,
};
use crate::time::{RangeMus, TimeMus};

use super::materialize::materialize;

pub fn trim_selection<M: EditModel>(
    model: &M,
    selection: &Selection<M::Lane, M::Object>,
    edge: Edge,
    to: TimeMus,
) -> Result<EditPlan<M>, EditError> {
    let view = materialize(model, selection)?;
    if view.mode != SelectionMode::Time {
        return Err(EditError::InvalidSelection(
            "trim_selection needs a time selection".into(),
        ));
    }
    let (lo, hi) = lane_bounds_intersection(model, &view.lanes);
    let eps = epsilon();
    let span = match edge {
        Edge::Start => {
            let max_start = view.span.end - eps;
            if max_start < lo {
                return Err(EditError::InvalidSpan);
            }
            RangeMus::new(clamp_time(to, lo, Some(max_start)), view.span.end)
        }
        Edge::End => {
            let min_end = view.span.start + eps;
            if hi.is_some_and(|hi| hi < min_end) {
                return Err(EditError::InvalidSpan);
            }
            RangeMus::new(view.span.start, clamp_time(to, min_end, hi))
        }
    }
    .map_err(|_| EditError::InvalidSpan)?;

    Ok(Plan {
        ops: Vec::new(),
        next: NextSelection::Time(TimeSelection {
            span,
            lanes: view.lanes,
        }),
    })
}

pub fn trim_object<M: EditModel>(
    model: &M,
    object: M::Object,
    edge: Edge,
    to: TimeMus,
) -> Result<EditPlan<M>, EditError> {
    let view = model
        .object(object)
        .ok_or_else(|| EditError::NotFound(format!("{object:?}")))?;
    if view.is_point() {
        return Err(EditError::PointNotTrimmable);
    }
    let limits = model.trim_limits(object);
    let span = match edge {
        Edge::Start => RangeMus::new(
            clamp_time(to, limits.min_start, Some(limits.max_start)),
            view.span.end,
        ),
        Edge::End => RangeMus::new(
            view.span.start,
            clamp_time(to, limits.min_end, limits.max_end),
        ),
    }
    .map_err(|_| EditError::InvalidSpan)?;
    if span.is_empty() {
        return Err(EditError::InvalidSpan);
    }
    if span == view.span {
        return Ok(Plan {
            ops: Vec::new(),
            next: NextSelection::Objects(vec![ObjectRef::Existing(object)]),
        });
    }

    Ok(Plan {
        ops: vec![EditOp::Trim { object, span }],
        next: NextSelection::Objects(vec![ObjectRef::Existing(object)]),
    })
}

pub fn trim_container<M: EditModel>(
    model: &M,
    edge: Edge,
    to: TimeMus,
) -> Result<EditPlan<M>, EditError> {
    let container = model.container().ok_or(EditError::NoContainer)?;
    let eps = epsilon();
    let limits = model.container_trim_limits().unwrap_or(TrimLimits {
        min_start: container.start,
        max_start: container.end - eps,
        min_end: container.start + eps,
        max_end: Some(container.end),
    });
    let span = match edge {
        Edge::Start => RangeMus::new(
            clamp_time(to, limits.min_start, Some(limits.max_start)),
            container.end,
        ),
        Edge::End => RangeMus::new(
            container.start,
            clamp_time(to, limits.min_end, limits.max_end),
        ),
    }
    .map_err(|_| EditError::InvalidSpan)?;
    if span.is_empty() {
        return Err(EditError::InvalidSpan);
    }
    if span == container {
        return Ok(Plan::empty(NextSelection::Unchanged));
    }
    Ok(Plan {
        ops: vec![EditOp::SetContainer { span }],
        next: NextSelection::Unchanged,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Start,
    End,
}

fn lane_bounds_intersection<M: EditModel>(
    model: &M,
    lanes: &BTreeSet<M::Lane>,
) -> (TimeMus, Option<TimeMus>) {
    let mut lo: Option<TimeMus> = None;
    let mut hi: Option<TimeMus> = None;
    for lane in lanes {
        let bounds = model.lane_bounds(*lane);
        lo = Some(lo.map_or(bounds.start, |value| value.max(bounds.start)));
        if let Some(end) = bounds.end {
            hi = Some(hi.map_or(end, |value| value.min(end)));
        }
    }
    (lo.unwrap_or_else(TimeMus::zero), hi)
}

fn clamp_time(value: TimeMus, min: TimeMus, max: Option<TimeMus>) -> TimeMus {
    let value = value.max(min);
    match max {
        Some(max) if min <= max => value.min(max),
        Some(_) => min,
        None => value,
    }
}
