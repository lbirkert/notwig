use super::materialize::{extract_from_view, materialize};
use crate::edit::{
    EditError, EditExtract, EditModel, EditOp, EditPlan, Materialized, NextSelection, ObjectExtent,
    Plan, Selection, SelectionMode, TimeSelection,
};

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
