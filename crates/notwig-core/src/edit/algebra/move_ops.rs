use crate::edit::{
    EditError, EditModel, EditOp, EditPlan, Materialized, NextSelection, ObjectRef, Plan,
    Selection, SelectionMode, TimeSelection,
};
use crate::time::{RangeMus, TimeMus};

use super::materialize::materialize;

pub fn move_selection<M: EditModel>(
    model: &M,
    selection: &Selection<M::Lane, M::Object>,
    delta: TimeMus,
) -> Result<EditPlan<M>, EditError> {
    let view = materialize(model, selection)?;
    let delta = match view.mode {
        SelectionMode::Objects => clamp_object_move(model, &view, delta),
        SelectionMode::Time => clamp_time_move(model, &view, delta),
    };
    if delta == TimeMus::zero() {
        return Ok(Plan::empty(NextSelection::Unchanged));
    }

    match view.mode {
        SelectionMode::Objects => {
            let ops = view
                .members
                .iter()
                .map(|member| {
                    Ok(EditOp::Move {
                        object: member.id,
                        lane: member.lane,
                        span: shifted(member.span, delta)?,
                    })
                })
                .collect::<Result<Vec<_>, EditError>>()?;
            Ok(Plan {
                ops,
                next: NextSelection::Objects(
                    view.members
                        .iter()
                        .map(|member| ObjectRef::Existing(member.id))
                        .collect(),
                ),
            })
        }
        SelectionMode::Time => {
            let dest = shifted(view.span, delta)?;
            let mut ops = Vec::new();
            for lane in &view.lanes {
                ops.push(EditOp::Erase {
                    lane: *lane,
                    span: view.span,
                });
            }
            for lane in &view.lanes {
                ops.push(EditOp::Erase {
                    lane: *lane,
                    span: dest,
                });
            }
            for fragment in &view.fragments {
                ops.push(EditOp::Place {
                    lane: fragment.lane,
                    span: RangeMus::new(
                        dest.start + fragment.offset,
                        dest.start + fragment.offset + fragment.length,
                    )
                    .map_err(|_| EditError::InvalidSpan)?,
                    kind: fragment.kind,
                    content: fragment.content.clone(),
                });
            }
            Ok(Plan {
                ops,
                next: NextSelection::Time(TimeSelection {
                    span: dest,
                    lanes: view.lanes.clone(),
                }),
            })
        }
    }
}

fn clamp_object_move<M: EditModel>(
    model: &M,
    view: &Materialized<M::Lane, M::Object, M::Content>,
    requested: TimeMus,
) -> TimeMus {
    let mut lower: Option<TimeMus> = None;
    let mut upper: Option<TimeMus> = None;
    for member in &view.members {
        let bounds = model.lane_bounds(member.lane);
        let lo = bounds.start - member.span.start;
        lower = Some(lower.map_or(lo, |value| value.max(lo)));
        if let Some(end) = bounds.end {
            let hi = end - member.span.end;
            upper = Some(upper.map_or(hi, |value| value.min(hi)));
        }
    }
    feasible_shift(requested, lower, upper)
}

fn clamp_time_move<M: EditModel>(
    model: &M,
    view: &Materialized<M::Lane, M::Object, M::Content>,
    requested: TimeMus,
) -> TimeMus {
    let mut lo: Option<TimeMus> = None;
    let mut hi: Option<TimeMus> = None;
    for lane in &view.lanes {
        let bounds = model.lane_bounds(*lane);
        lo = Some(lo.map_or(bounds.start, |value| value.max(bounds.start)));
        if let Some(end) = bounds.end {
            hi = Some(hi.map_or(end, |value| value.min(end)));
        }
    }
    feasible_shift(
        requested,
        lo.map(|lo| lo - view.span.start),
        hi.map(|hi| hi - view.span.end),
    )
}

fn feasible_shift(requested: TimeMus, lower: Option<TimeMus>, upper: Option<TimeMus>) -> TimeMus {
    if let (Some(lo), Some(hi)) = (lower, upper)
        && lo > hi
    {
        return TimeMus::zero();
    }
    let mut value = requested;
    if let Some(lo) = lower {
        value = value.max(lo);
    }
    if let Some(hi) = upper {
        value = value.min(hi);
    }
    value
}

pub fn relocate_objects<M: EditModel>(
    model: &M,
    moves: &[ObjectMove<M::Lane, M::Object>],
) -> Result<EditPlan<M>, EditError> {
    let lanes: std::collections::BTreeSet<M::Lane> = model.lanes().into_iter().collect();
    let mut ops = Vec::with_capacity(moves.len());
    for mv in moves {
        let view = model
            .object(mv.object)
            .ok_or_else(|| EditError::NotFound(format!("{:?}", mv.object)))?;
        if !lanes.contains(&mv.lane) {
            return Err(EditError::InvalidSelection("missing target lane".into()));
        }
        if !model.accepts(mv.lane, view.kind) {
            return Err(EditError::IncompatibleLane(view.kind));
        }
        if mv.span.is_empty() && !view.is_point() {
            return Err(EditError::InvalidSpan);
        }
        let bounds = model.lane_bounds(mv.lane);
        if mv.span.start < bounds.start || bounds.end.is_some_and(|end| mv.span.end > end) {
            return Err(EditError::InvalidSpan);
        }
        ops.push(EditOp::Move {
            object: mv.object,
            lane: mv.lane,
            span: mv.span,
        });
    }
    Ok(Plan {
        ops,
        next: NextSelection::Unchanged,
    })
}

fn shifted(span: RangeMus, delta: TimeMus) -> Result<RangeMus, EditError> {
    RangeMus::new(span.start + delta, span.end + delta).map_err(|_| EditError::InvalidSpan)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectMove<L, O> {
    pub object: O,
    pub lane: L,
    pub span: RangeMus,
}
