use std::collections::BTreeSet;

use crate::edit::{
    EditError, EditModel, EditOp, EditPlan, Extract, NextSelection, ObjectRef, Plan, Selection,
    SelectionMode, TimeSelection,
};
use crate::time::{RangeMus, TimeMus};

use super::materialize::{extract_from_view, materialize};

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
