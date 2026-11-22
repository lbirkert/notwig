use std::collections::BTreeSet;

use super::{
    EditError, EditExtract, EditMaterialized, EditModel, EditOp, EditPlan, Extract, Extracted,
    Fragment, Materialized, NextSelection, ObjectExtent, ObjectRef, ObjectView, Plan, Selection,
    SelectionMode, TimeSelection, TrimLimits, epsilon,
};
use crate::time::{RangeMus, TimeMus};

pub fn materialize<M: EditModel>(
    model: &M,
    selection: &Selection<M::Lane, M::Object>,
) -> Result<EditMaterialized<M>, EditError> {
    match selection.mode() {
        Some(SelectionMode::Objects) => materialize_objects(model, &selection.objects),
        Some(SelectionMode::Time) => {
            let time = selection
                .time
                .as_ref()
                .ok_or_else(|| EditError::InvalidSelection("missing time selection".into()))?;
            materialize_time(model, time)
        }
        None => Err(EditError::InvalidSelection("selection is empty".into())),
    }
}

fn materialize_objects<M: EditModel>(
    model: &M,
    objects: &BTreeSet<M::Object>,
) -> Result<EditMaterialized<M>, EditError> {
    let mut members: Vec<ObjectView<M::Lane, M::Object>> = objects
        .iter()
        .filter_map(|object| model.object(*object))
        .collect();
    if members.is_empty() {
        return Err(EditError::InvalidSelection("no live objects".into()));
    }
    members.sort_by(|a, b| {
        a.lane
            .cmp(&b.lane)
            .then(a.span.start.cmp(&b.span.start))
            .then(a.id.cmp(&b.id))
    });

    let start = members
        .iter()
        .map(|view| view.span.start)
        .min()
        .ok_or_else(|| EditError::InvalidSelection("no live objects".into()))?;
    let end = members
        .iter()
        .map(|view| match view.extent {
            ObjectExtent::Span => view.span.end,
            ObjectExtent::Point => view.span.start + epsilon(),
        })
        .max()
        .ok_or_else(|| EditError::InvalidSelection("no live objects".into()))?;
    let span =
        RangeMus::new(start, end.max(start + epsilon())).map_err(|_| EditError::InvalidSpan)?;

    let mut fragments = Vec::with_capacity(members.len());
    let mut lanes = BTreeSet::new();
    for view in &members {
        let content = model
            .extract(view.id, view.span)
            .map_err(|err| EditError::InvalidExtraction(err.to_string()))?;
        lanes.insert(view.lane);
        fragments.push(Fragment {
            lane: view.lane,
            offset: view.span.start - span.start,
            length: view.span.length(),
            kind: view.kind,
            content,
        });
    }

    Ok(Materialized {
        mode: SelectionMode::Objects,
        span,
        lanes,
        members,
        fragments,
    })
}

fn materialize_time<M: EditModel>(
    model: &M,
    time: &TimeSelection<M::Lane>,
) -> Result<EditMaterialized<M>, EditError> {
    if time.span.is_empty() {
        return Err(EditError::InvalidSelection(
            "time selection is empty".into(),
        ));
    }
    if time.lanes.is_empty() {
        return Err(EditError::InvalidSelection(
            "time selection has no lanes".into(),
        ));
    }

    let mut members = Vec::new();
    for lane in &time.lanes {
        for view in model.objects(*lane) {
            let intersects = match view.extent {
                ObjectExtent::Span => view.span.intersects(time.span),
                ObjectExtent::Point => {
                    time.span.start <= view.span.start && view.span.start < time.span.end
                }
            };
            if intersects {
                members.push(view);
            }
        }
    }
    members.sort_by(|a, b| {
        a.lane
            .cmp(&b.lane)
            .then(a.span.start.cmp(&b.span.start))
            .then(a.id.cmp(&b.id))
    });

    let mut fragments = Vec::new();
    for view in &members {
        let window = match view.extent {
            ObjectExtent::Span => {
                let start = view.span.start.max(time.span.start);
                let end = view.span.end.min(time.span.end);
                if end <= start {
                    continue;
                }
                RangeMus::new(start, end).map_err(|_| EditError::InvalidSpan)?
            }
            ObjectExtent::Point => view.span,
        };
        let content = model
            .extract(view.id, window)
            .map_err(|err| EditError::InvalidExtraction(err.to_string()))?;
        fragments.push(Fragment {
            lane: view.lane,
            offset: window.start - time.span.start,
            length: window.length(),
            kind: view.kind,
            content,
        });
    }

    Ok(Materialized {
        mode: SelectionMode::Time,
        span: time.span,
        lanes: time.lanes.clone(),
        members,
        fragments,
    })
}

pub fn copy<M: EditModel>(
    model: &M,
    selection: &Selection<M::Lane, M::Object>,
) -> Result<EditExtract<M>, EditError> {
    let view = materialize(model, selection)?;
    Ok(extract_from_view(model, &view))
}

fn extract_from_view<M: EditModel>(
    model: &M,
    view: &Materialized<M::Lane, M::Object, M::Content>,
) -> Extract<M::Lane, M::Content> {
    Extract {
        domain: model.editor(),
        mode: view.mode,
        length: view.span.length(),
        lanes: view.lanes.iter().copied().collect(),
        fragments: view
            .fragments
            .iter()
            .map(|fragment| Extracted {
                lane: fragment.lane,
                offset: fragment.offset,
                length: fragment.length,
                kind: fragment.kind,
                content: fragment.content.clone(),
            })
            .collect(),
    }
}

pub fn split<M: EditModel>(
    model: &M,
    selection: &Selection<M::Lane, M::Object>,
) -> Result<EditPlan<M>, EditError> {
    let time = selection
        .time
        .as_ref()
        .ok_or_else(|| EditError::InvalidSelection("split needs a time selection".into()))?;
    if !time.span.is_empty() {
        return Err(EditError::InvalidSelection(
            "split needs a zero-width time selection".into(),
        ));
    }
    if time.lanes.is_empty() {
        return Err(EditError::InvalidSelection(
            "time selection has no lanes".into(),
        ));
    }
    let at = time.span.start;
    let mut ops = Vec::new();
    for lane in &time.lanes {
        for view in model.objects(*lane) {
            if view.extent == ObjectExtent::Span && view.span.start < at && at < view.span.end {
                ops.push(EditOp::Split {
                    object: view.id,
                    at,
                });
            }
        }
    }
    Ok(Plan {
        ops,
        next: NextSelection::Time(time.clone()),
    })
}

pub fn delete<M: EditModel>(
    model: &M,
    selection: &Selection<M::Lane, M::Object>,
) -> Result<EditPlan<M>, EditError> {
    if selection.objects.is_empty()
        && selection
            .time
            .as_ref()
            .is_some_and(|time| time.span.is_empty())
    {
        return split(model, selection);
    }
    let view = materialize(model, selection)?;
    Ok(delete_from_view(&view))
}

fn delete_from_view<L: Ord + Copy, O: Ord + Copy, C>(
    view: &Materialized<L, O, C>,
) -> Plan<L, O, C> {
    match view.mode {
        SelectionMode::Objects => Plan {
            ops: view
                .members
                .iter()
                .map(|member| EditOp::Remove { object: member.id })
                .collect(),
            next: NextSelection::None,
        },
        SelectionMode::Time => Plan {
            ops: view
                .lanes
                .iter()
                .map(|lane| EditOp::Erase {
                    lane: *lane,
                    span: view.span,
                })
                .collect(),
            next: NextSelection::Time(TimeSelection {
                span: view.span,
                lanes: view.lanes.clone(),
            }),
        },
    }
}

pub fn cut<M: EditModel>(
    model: &M,
    selection: &Selection<M::Lane, M::Object>,
) -> Result<(EditExtract<M>, EditPlan<M>), EditError> {
    let view = materialize(model, selection)?;
    Ok((extract_from_view(model, &view), delete_from_view(&view)))
}

pub fn paste<M: EditModel>(
    model: &M,
    extract: &Extract<M::Lane, M::Content>,
    at: TimeMus,
) -> Result<EditPlan<M>, EditError> {
    if extract.domain != model.editor() {
        return Err(EditError::InvalidSelection(
            "clipboard belongs to another editor".into(),
        ));
    }

    let lanes: BTreeSet<M::Lane> = model.lanes().into_iter().collect();
    for fragment in &extract.fragments {
        if !lanes.contains(&fragment.lane) {
            return Err(EditError::InvalidSelection("missing paste lane".into()));
        }
        if !model.accepts(fragment.lane, fragment.kind) {
            return Err(EditError::IncompatibleLane(fragment.kind));
        }
    }

    if at < TimeMus::zero() {
        return Err(EditError::InvalidPasteTarget);
    }

    let mut lo: Option<TimeMus> = None;
    let mut hi: Option<TimeMus> = None;
    for lane in &extract.lanes {
        let bounds = model.lane_bounds(*lane);
        lo = Some(lo.map_or(bounds.start, |value| value.max(bounds.start)));
        if let Some(end) = bounds.end {
            hi = Some(hi.map_or(end, |value| value.min(end)));
        }
    }
    if lo.is_some_and(|lo| at < lo) {
        return Err(EditError::InvalidPasteTarget);
    }
    if let Some(hi) = hi
        && at + extract.length > hi
    {
        return Err(EditError::InvalidPasteTarget);
    }

    let dest = RangeMus::new(at, at + extract.length).map_err(|_| EditError::InvalidSpan)?;
    let mut ops = Vec::new();
    match extract.mode {
        SelectionMode::Time => {
            for lane in &extract.lanes {
                ops.push(EditOp::Erase {
                    lane: *lane,
                    span: dest,
                });
            }
        }
        SelectionMode::Objects => {
            for fragment in &extract.fragments {
                ops.push(EditOp::Erase {
                    lane: fragment.lane,
                    span: RangeMus::new(
                        at + fragment.offset,
                        at + fragment.offset + fragment.length,
                    )
                    .map_err(|_| EditError::InvalidSpan)?,
                });
            }
        }
    }

    let mut created = Vec::with_capacity(extract.fragments.len());
    for fragment in &extract.fragments {
        let span = RangeMus::new(at + fragment.offset, at + fragment.offset + fragment.length)
            .map_err(|_| EditError::InvalidSpan)?;
        created.push(ObjectRef::Created(ops.len()));
        ops.push(EditOp::Place {
            lane: fragment.lane,
            span,
            kind: fragment.kind,
            content: fragment.content.clone(),
        });
    }

    let next = match extract.mode {
        SelectionMode::Time => NextSelection::Time(TimeSelection {
            span: dest,
            lanes: extract.lanes.iter().copied().collect(),
        }),
        SelectionMode::Objects => NextSelection::Objects(created),
    };

    Ok(Plan { ops, next })
}

pub fn duplicate<M: EditModel>(
    model: &M,
    selection: &Selection<M::Lane, M::Object>,
) -> Result<EditPlan<M>, EditError> {
    let view = materialize(model, selection)?;
    let extract = extract_from_view(model, &view);
    paste(model, &extract, view.span.end)
}

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

pub fn relocate_objects<M: EditModel>(
    model: &M,
    moves: &[ObjectMove<M::Lane, M::Object>],
) -> Result<EditPlan<M>, EditError> {
    let lanes: BTreeSet<M::Lane> = model.lanes().into_iter().collect();
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

fn shifted(span: RangeMus, delta: TimeMus) -> Result<RangeMus, EditError> {
    RangeMus::new(span.start + delta, span.end + delta).map_err(|_| EditError::InvalidSpan)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectMove<L, O> {
    pub object: O,
    pub lane: L,
    pub span: RangeMus,
}
