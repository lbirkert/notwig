use std::collections::{BTreeMap, BTreeSet};

use notwig_core::edit::{
    Edge, EditError, EditModel, EditOp, EditorKind, Extract, LaneBounds, NextSelection,
    ObjectExtent, ObjectKind, ObjectMove, ObjectView, Plan, Selection, SelectionMode,
    TimeSelection, TrimLimits, copy, cut, delete, duplicate, materialize, move_selection, paste,
    relocate_objects, trim_object, trim_selection,
};
use notwig_core::time::{RangeMus, TimeMus};

const LANE: &str = "lane";

#[derive(Clone, Debug, PartialEq)]
struct TapeContent {
    token: u32,
    source_start: i64,
}

#[derive(Clone, Debug)]
struct TapeObject {
    id: u32,
    span: RangeMus,
    token: u32,
    source_start: i64,
}

#[derive(Clone, Debug, Default)]
struct TapeModel {
    objects: BTreeMap<u32, TapeObject>,
}

impl TapeModel {
    fn new(objects: impl IntoIterator<Item = (u32, i64, i64)>) -> Self {
        let mut model = Self::default();
        for (id, start, end) in objects {
            model.objects.insert(
                id,
                TapeObject {
                    id,
                    span: span(start, end),
                    token: id,
                    source_start: 0,
                },
            );
        }
        model
    }

    fn snapshot(&self) -> Vec<(i64, i64, u32, i64)> {
        let mut out: Vec<_> = self
            .objects
            .values()
            .map(|object| {
                (
                    ticks(object.span.start),
                    ticks(object.span.end),
                    object.token,
                    object.source_start,
                )
            })
            .collect();
        out.sort_unstable();
        out
    }
}

impl EditModel for TapeModel {
    type Lane = &'static str;
    type Object = u32;
    type Content = TapeContent;

    fn editor(&self) -> EditorKind {
        EditorKind::Arranger
    }

    fn lanes(&self) -> Vec<Self::Lane> {
        vec![LANE]
    }

    fn lane_bounds(&self, _lane: Self::Lane) -> LaneBounds {
        LaneBounds {
            start: TimeMus::zero(),
            end: None,
        }
    }

    fn objects(&self, _lane: Self::Lane) -> Vec<ObjectView<Self::Lane, Self::Object>> {
        let mut out: Vec<_> = self
            .objects
            .values()
            .map(|object| ObjectView {
                id: object.id,
                lane: LANE,
                span: object.span,
                kind: ObjectKind::ClipMidi,
                extent: ObjectExtent::Span,
            })
            .collect();
        out.sort_by_key(|view| view.span.start);
        out
    }

    fn object(&self, object: Self::Object) -> Option<ObjectView<Self::Lane, Self::Object>> {
        self.objects.get(&object).map(|object| ObjectView {
            id: object.id,
            lane: LANE,
            span: object.span,
            kind: ObjectKind::ClipMidi,
            extent: ObjectExtent::Span,
        })
    }

    fn trim_limits(&self, object: Self::Object) -> TrimLimits {
        let view = self.object(object).expect("object exists");
        TrimLimits {
            min_start: TimeMus::zero(),
            max_start: view.span.end - mus(1),
            min_end: view.span.start + mus(1),
            max_end: None,
        }
    }

    fn extract(&self, object: Self::Object, window: RangeMus) -> Result<Self::Content, EditError> {
        let object = self
            .objects
            .get(&object)
            .ok_or_else(|| EditError::NotFound("missing".into()))?;
        Ok(TapeContent {
            token: object.token,
            source_start: object.source_start + ticks(window.start) - ticks(object.span.start),
        })
    }

    fn accepts(&self, _lane: Self::Lane, _kind: ObjectKind) -> bool {
        true
    }
}

fn span(start: i64, end: i64) -> RangeMus {
    RangeMus::new(TimeMus::new(start, 1), TimeMus::new(end, 1)).unwrap()
}

fn mus(ticks: i64) -> TimeMus {
    TimeMus::new(ticks, 16)
}

fn ticks(time: TimeMus) -> i64 {
    (time.as_f64() * 16.0).round() as i64
}

fn selection_time(start: i64, end: i64) -> Selection<&'static str, u32> {
    Selection::time(span(start, end), [LANE])
}

fn apply(model: &mut TapeModel, plan: &Plan<&'static str, u32, TapeContent>) {
    let mut next_id = model.objects.keys().copied().max().unwrap_or(0) + 1;
    for op in &plan.ops {
        match op {
            EditOp::SetContainer { .. } => {}
            EditOp::Erase { span, .. } => erase(model, *span),
            EditOp::Remove { object } => {
                model.objects.remove(object);
            }
            EditOp::Move { object, span, .. } => {
                if let Some(target) = model.objects.get_mut(object) {
                    target.span = *span;
                }
            }
            EditOp::Trim { object, span } => {
                if let Some(target) = model.objects.get_mut(object) {
                    let delta = ticks(span.start) - ticks(target.span.start);
                    target.source_start += delta;
                    target.span = *span;
                }
            }
            EditOp::Place { span, content, .. } => {
                model.objects.insert(
                    next_id,
                    TapeObject {
                        id: next_id,
                        span: *span,
                        token: content.token,
                        source_start: content.source_start,
                    },
                );
                next_id += 1;
            }
        }
    }
}

fn erase(model: &mut TapeModel, range: RangeMus) {
    let mut out = Vec::new();
    let mut objs: Vec<_> = model.objects.values().cloned().collect();
    objs.sort_by_key(|object| object.id);
    for object in objs {
        if !object.span.intersects(range) {
            out.push(object);
            continue;
        }
        let covers_start = object.span.start < range.start;
        let covers_end = object.span.end > range.end;
        match (covers_start, covers_end) {
            (false, false) => {}
            (false, true) => {
                let mut right = object.clone();
                right.span = RangeMus::new(range.end, object.span.end).unwrap();
                right.source_start += ticks(range.end) - ticks(object.span.start);
                out.push(right);
            }
            (true, false) => {
                let mut left = object.clone();
                left.span = RangeMus::new(object.span.start, range.start).unwrap();
                out.push(left);
            }
            (true, true) => {
                let mut left = object.clone();
                left.span = RangeMus::new(object.span.start, range.start).unwrap();
                out.push(left);
                let mut right = object.clone();
                right.span = RangeMus::new(range.end, object.span.end).unwrap();
                right.source_start += ticks(range.end) - ticks(object.span.start);
                out.push(right);
            }
        }
    }
    model.objects.clear();
    for object in out {
        model.objects.insert(object.id, object);
    }
}

#[test]
fn time_copy_clips_and_preserves_gaps() {
    let model = TapeModel::new([(1, 2, 5), (2, 6, 7)]);
    let extract = copy(&model, &selection_time(3, 7)).unwrap();
    assert_eq!(extract.mode, SelectionMode::Time);
    assert_eq!(ticks(extract.length), 64);
    assert_eq!(extract.fragments.len(), 2);
    assert_eq!(ticks(extract.fragments[0].offset), 0);
    assert_eq!(ticks(extract.fragments[0].length), 32);
    assert_eq!(ticks(extract.fragments[1].offset), 48);
    assert_eq!(ticks(extract.fragments[1].length), 16);
}

#[test]
fn time_delete_splits_and_removes() {
    let mut model = TapeModel::new([(1, 2, 5), (2, 6, 7)]);
    let plan = delete(&model, &selection_time(3, 7)).unwrap();
    apply(&mut model, &plan);
    assert_eq!(model.snapshot(), vec![(32, 48, 1, 0)]);
}

#[test]
fn time_move_clips_and_leaves_outside_parts() {
    let mut model = TapeModel::new([(1, 2, 5), (2, 6, 7)]);
    let plan = move_selection(&model, &selection_time(3, 7), TimeMus::new(2, 1)).unwrap();
    apply(&mut model, &plan);
    assert_eq!(
        model.snapshot(),
        vec![(32, 48, 1, 0), (80, 112, 1, 16), (128, 144, 2, 0)]
    );
}

#[test]
fn duplicate_objects_places_after_bounding_end() {
    let mut model = TapeModel::new([(1, 2, 5), (2, 6, 7)]);
    let selection = Selection::objects([1, 2]);
    let plan = duplicate(&model, &selection).unwrap();
    apply(&mut model, &plan);
    assert_eq!(
        model.snapshot(),
        vec![
            (32, 80, 1, 0),
            (96, 112, 2, 0),
            (112, 160, 1, 0),
            (176, 192, 2, 0),
        ]
    );
}

#[test]
fn time_paste_clears_the_whole_destination() {
    let mut model = TapeModel::new([(1, 2, 5), (2, 6, 7), (3, 10, 14)]);
    let extract = copy(&model, &selection_time(3, 7)).unwrap();
    let plan = paste(&model, &extract, TimeMus::new(10, 1)).unwrap();
    apply(&mut model, &plan);
    assert_eq!(
        model.snapshot(),
        vec![
            (32, 80, 1, 0),
            (96, 112, 2, 0),
            (160, 192, 1, 16),
            (208, 224, 2, 0),
        ]
    );
}

#[test]
fn cut_is_copy_then_delete() {
    let model = TapeModel::new([(1, 2, 5), (2, 6, 7)]);
    let selection = selection_time(3, 7);
    let (extract, plan) = cut(&model, &selection).unwrap();
    assert_eq!(extract, copy(&model, &selection).unwrap());
    assert_eq!(plan, delete(&model, &selection).unwrap());
}

#[test]
fn trim_selection_changes_only_the_time_selection() {
    let model = TapeModel::new([(1, 2, 5)]);
    let plan = trim_selection(
        &model,
        &selection_time(3, 7),
        Edge::Start,
        TimeMus::new(4, 1),
    )
    .unwrap();
    assert_eq!(plan.ops.len(), 0);
    match plan.next {
        NextSelection::Time(time) => {
            assert_eq!(time.span, span(4, 7));
        }
        _ => panic!("expected time selection"),
    }
}

#[test]
fn object_trim_keeps_the_opposite_edge() {
    let model = TapeModel::new([(1, 2, 5)]);
    let plan = trim_object(&model, 1, Edge::Start, TimeMus::new(3, 1)).unwrap();
    assert_eq!(plan.ops.len(), 1);
    match &plan.ops[0] {
        EditOp::Trim { span: trimmed, .. } => assert_eq!(*trimmed, span(3, 5)),
        _ => panic!("expected trim"),
    }
}

#[test]
fn relocate_objects_keeps_identity() {
    let model = TapeModel::new([(1, 2, 5)]);
    let plan = relocate_objects(
        &model,
        &[ObjectMove {
            object: 1,
            lane: LANE,
            span: span(6, 9),
        }],
    )
    .unwrap();
    match &plan.ops[0] {
        EditOp::Move {
            object,
            lane,
            span: moved,
        } => {
            assert_eq!(*object, 1);
            assert_eq!(*lane, LANE);
            assert_eq!(*moved, span(6, 9));
        }
        _ => panic!("expected move"),
    }
}

#[test]
fn materialize_time_includes_only_intersecting_spans() {
    let model = TapeModel::new([(1, 2, 3), (2, 5, 5)]);
    let view = materialize(&model, &Selection::time(span(2, 5), [LANE])).unwrap();
    assert_eq!(view.members.len(), 1);
    assert_eq!(view.members[0].id, 1);
}

#[test]
fn plans_are_deterministic() {
    let model = TapeModel::new([(1, 2, 5), (2, 6, 7)]);
    let selection = selection_time(3, 7);
    assert_eq!(
        move_selection(&model, &selection, TimeMus::new(2, 1)).unwrap(),
        move_selection(&model, &selection, TimeMus::new(2, 1)).unwrap()
    );
}

#[test]
fn paste_rejects_a_foreign_clipboard() {
    let model = TapeModel::new([(1, 2, 5)]);
    let extract: Extract<&'static str, TapeContent> = Extract {
        domain: EditorKind::Midi,
        mode: SelectionMode::Objects,
        length: TimeMus::new(1, 1),
        lanes: vec![LANE],
        fragments: Vec::new(),
    };
    assert!(paste(&model, &extract, TimeMus::zero()).is_err());
}

#[test]
fn time_selection_move_clamps_to_lane_start() {
    let mut model = TapeModel::new([(1, 2, 5)]);
    let plan = move_selection(&model, &selection_time(3, 7), TimeMus::new(-10, 1)).unwrap();
    apply(&mut model, &plan);
    assert_eq!(model.snapshot(), vec![(0, 32, 1, 16)]);
}

#[test]
fn empty_time_selection_is_rejected() {
    let model = TapeModel::new([(1, 2, 5)]);
    let selection = Selection {
        objects: BTreeSet::default(),
        time: Some(TimeSelection {
            span: span(3, 3),
            lanes: [LANE].into_iter().collect(),
        }),
    };
    assert!(copy(&model, &selection).is_err());
}

mod properties {
    use super::*;
    use notwig_core::edit::SelectionMode;
    use proptest::prelude::*;

    fn disjoint_model() -> impl Strategy<Value = TapeModel> {
        proptest::collection::vec((0i64..56, 1i64..5), 0..10).prop_map(|spans| {
            let mut spans = spans;
            spans.sort_unstable();
            let mut out = Vec::new();
            let mut last_end = -1;
            for (start, len) in spans {
                if start >= last_end {
                    out.push((start, start + len));
                    last_end = start + len;
                }
            }
            TapeModel::new(
                out.into_iter()
                    .enumerate()
                    .map(|(index, (start, end))| ((index + 1) as u32, start, end)),
            )
        })
    }

    fn model_and_selection() -> impl Strategy<Value = (TapeModel, Selection<&'static str, u32>)> {
        disjoint_model().prop_flat_map(|model| {
            let count = u32::try_from(model.objects.len()).unwrap_or(0).max(1);
            let selection = prop_oneof![
                (0i64..56, 1i64..8).prop_map(|(start, len)| selection_time(start, start + len)),
                proptest::collection::vec(1u32..=count, 1..=3).prop_map(Selection::objects),
            ];
            (Just(model), selection)
        })
    }

    fn operation_delta() -> TimeMus {
        TimeMus::new(1, 1)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]

        #[test]
        fn operations_are_deterministic((model, selection) in model_and_selection()) {
            prop_assert_eq!(copy(&model, &selection).ok(), copy(&model, &selection).ok());
            prop_assert_eq!(delete(&model, &selection).ok(), delete(&model, &selection).ok());
            prop_assert_eq!(duplicate(&model, &selection).ok(), duplicate(&model, &selection).ok());
            prop_assert_eq!(
                move_selection(&model, &selection, operation_delta()).ok(),
                move_selection(&model, &selection, operation_delta()).ok()
            );
        }

        #[test]
        fn every_plan_span_is_valid((model, selection) in model_and_selection()) {
            let plans = [
                delete(&model, &selection).ok(),
                duplicate(&model, &selection).ok(),
                move_selection(&model, &selection, operation_delta()).ok(),
            ];
            for plan in plans.into_iter().flatten() {
                for op in &plan.ops {
                    let span = match op {
                        EditOp::SetContainer { span }
                        | EditOp::Erase { span, .. }
                        | EditOp::Move { span, .. }
                        | EditOp::Trim { span, .. }
                        | EditOp::Place { span, .. } => *span,
                        EditOp::Remove { .. } => continue,
                    };
                    prop_assert!(span.start <= span.end);
                    prop_assert!(span.start >= TimeMus::zero());
                }
            }
        }

        #[test]
        fn cut_is_copy_then_delete((model, selection) in model_and_selection()) {
            let cut_result = cut(&model, &selection);
            let copy_result = copy(&model, &selection);
            let delete_result = delete(&model, &selection);
            if let Ok((extract, plan)) = cut_result {
                prop_assert_eq!(Ok(extract), copy_result);
                prop_assert_eq!(Ok(plan), delete_result);
            } else {
                prop_assert!(copy_result.is_err());
                prop_assert!(delete_result.is_err());
            }
        }

        #[test]
        fn time_trim_changes_no_content((model, selection) in model_and_selection()) {
            if selection.mode() != Some(SelectionMode::Time) {
                return Ok(());
            }
            let before = model.snapshot();
            if let Ok(plan) = trim_selection(&model, &selection, Edge::Start, TimeMus::new(1, 1)) {
                let mut after = model.clone();
                apply(&mut after, &plan);
                prop_assert_eq!(after.snapshot(), before);
            }
        }

        #[test]
        fn time_move_preserves_span_length((model, selection) in model_and_selection()) {
            let Some(time) = &selection.time else { return Ok(()); };
            if let Ok(plan) = move_selection(&model, &selection, operation_delta()) {
                match plan.next {
                    NextSelection::Time(next) => {
                        prop_assert_eq!(next.span.length(), time.span.length());
                    }
                    NextSelection::Unchanged => {}
                    _ => prop_assert!(false, "time move returned a non-time selection"),
                }
            }
        }
    }
}
