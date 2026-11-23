use std::collections::BTreeSet;

use crate::edit::{
    EditError, EditExtract, EditMaterialized, EditModel, Extract, Extracted, Fragment,
    Materialized, ObjectExtent, ObjectView, Selection, SelectionMode, TimeSelection, epsilon,
};
use crate::time::RangeMus;

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

pub(super) fn extract_from_view<M: EditModel>(
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
