use notwig_core::edit::{self, EditOp, NextSelection, Plan, Selection};
use notwig_core::sequencer::{AudioClipData, AudioSlice};
use notwig_core::{ClipId, ClipPayload, Db, RangeMus, SampleRate, SliceId, TempoMap, TimeMus};
use notwig_ui::test_api::clip_editor::{
    ClipEdge, ClipLane, SliceDrag, SliceDragMode, SliceLowered, SliceModel, SliceSelection,
    clamp_slice_start, clip_edge_drag, delete_range, drag_origin_start, edge_preview_range,
    emit_slices, geometry_duration, geometry_source_end, geometry_source_start, geometry_start,
    group_move_shift, handle_clip_editor_actions, lower_slice_plan, set_drag_origin_start,
    slice_drag, slice_geometry, slice_model, source_duration,
};
use notwig_ui::{ClipEditorState, EditFocus, NotwigUi, UiIntent};
use uuid::Uuid;

fn map() -> TempoMap {
    TempoMap {
        tempo_bpm: 120.0,
        sample_rate: SampleRate::DEFAULT,
    }
}

fn slice_at(id: u128, start: i64, source_start: u64, source_end: u64) -> AudioSlice {
    AudioSlice {
        id: SliceId::from_uuid(Uuid::from_u128(id)),
        start: TimeMus::new(start, 1),
        source_start_sample: source_start,
        source_end_sample: source_end,
        gain_db: Db(0.0),
        asset_id: None,
    }
}

fn slice() -> AudioSlice {
    slice_at(7, 1, 0, 48_000)
}

fn audio() -> AudioClipData {
    AudioClipData {
        asset_id: notwig_core::AssetId::from_uuid(Uuid::from_u128(9)),
        source_start_sample: 0,
        source_end_sample: 48_000,
        gain_db: Db(0.0),
        fade_in_samples: 0,
        fade_out_samples: 0,
        fade_shape: notwig_core::sequencer::FadeShape::Linear,
        slices: vec![slice()],
        slices_enabled: true,
    }
}

fn drag(mode: SliceDragMode) -> SliceDrag {
    slice_drag(mode, slice().id, TimeMus::new(1, 1), 0, 48_000)
}

#[test]
fn node_and_bar_selection_are_mutually_exclusive() {
    let mut state = ClipEditorState::new(ClipId::nil());
    state.select_node(slice().id, false);
    assert!(!state.selected_is_empty());
    assert!(state.bar_selection().is_none());

    state.select_bar(RangeMus::new(TimeMus::new(1, 1), TimeMus::new(2, 1)).unwrap());
    assert!(state.selected_is_empty());
    assert!(state.bar_selection().is_some());

    state.select_node(slice().id, false);
    assert!(!state.selected_is_empty());
    assert!(state.bar_selection().is_none());
}

#[test]
fn additive_node_selection_preserves_bar_selection_and_uses_node_mode() {
    let mut state = ClipEditorState::new(ClipId::nil());
    let range = RangeMus::new(TimeMus::new(1, 1), TimeMus::new(3, 1)).unwrap();
    state.select_bar(range);
    state.select_node(slice().id, true);
    assert_eq!(state.bar_selection(), Some(range));
    assert!(state.selected_contains(slice().id));
    assert_eq!(
        state.selection_mode(),
        Some(notwig_core::edit::SelectionMode::Objects)
    );
}

#[test]
fn move_snaps_anchor_and_preserves_group_offsets() {
    let drag = drag(SliceDragMode::Move);
    let starts = [TimeMus::new(0, 1), TimeMus::new(1, 1)];
    let shift = group_move_shift(
        drag,
        1.1,
        TimeMus::new(1, 4),
        false,
        TimeMus::zero(),
        &starts,
    );
    assert_eq!(shift, TimeMus::new(1, 1));
}

#[test]
fn move_clamps_group_at_timeline_start() {
    let mut drag = drag(SliceDragMode::Move);
    set_drag_origin_start(&mut drag, TimeMus::new(1, 1));
    let starts = [TimeMus::new(1, 1), TimeMus::new(1, 2)];
    let shift = group_move_shift(
        drag,
        -10.0,
        TimeMus::new(1, 4),
        false,
        TimeMus::zero(),
        &starts,
    );
    let anchor = clamp_slice_start(drag_origin_start(&drag) + shift, TimeMus::zero());
    assert_eq!(anchor, TimeMus::new(1, 2));
}

#[test]
fn move_allows_events_past_the_clip_end() {
    let mut drag = drag(SliceDragMode::Move);
    set_drag_origin_start(&mut drag, TimeMus::new(1, 1));
    let starts = [TimeMus::new(1, 1), TimeMus::new(2, 1)];
    let shift = group_move_shift(
        drag,
        10.0,
        TimeMus::new(1, 4),
        true,
        TimeMus::zero(),
        &starts,
    );
    assert_eq!(drag_origin_start(&drag) + shift, TimeMus::new(11, 1));
}

#[test]
fn alt_bypasses_move_snap() {
    let drag = drag(SliceDragMode::Move);
    let starts = [TimeMus::new(1, 1)];
    let shift = group_move_shift(
        drag,
        1.1 / 3.0,
        TimeMus::new(1, 4),
        true,
        TimeMus::zero(),
        &starts,
    );
    assert!((shift.as_f64() - 1.1 / 3.0).abs() < 1e-6);
}

#[test]
fn trim_start_keeps_right_edge_fixed() {
    let geometry = slice_geometry(
        &slice(),
        Some(drag(SliceDragMode::TrimStart)),
        1.0,
        TimeMus::zero(),
        &map(),
        TimeMus::new(1, 4),
        false,
        TimeMus::zero(),
        true,
    );
    assert_eq!(geometry_start(&geometry), TimeMus::new(2, 1));
    assert_eq!(geometry_source_start(&geometry), 24_000);
    assert_eq!(
        geometry_start(&geometry) + geometry_duration(&geometry, &map()),
        TimeMus::new(3, 1)
    );
}

#[test]
fn trim_end_extends_source_and_clamps_non_empty() {
    let geometry = slice_geometry(
        &slice(),
        Some(drag(SliceDragMode::TrimEnd)),
        1.0,
        TimeMus::zero(),
        &map(),
        TimeMus::new(1, 4),
        false,
        TimeMus::zero(),
        true,
    );
    assert_eq!(geometry_source_end(&geometry), 72_000);
    assert_eq!(geometry_duration(&geometry, &map()), TimeMus::new(3, 1));

    let collapsed = slice_geometry(
        &slice(),
        Some(drag(SliceDragMode::TrimEnd)),
        -100.0,
        TimeMus::zero(),
        &map(),
        TimeMus::new(1, 4),
        false,
        TimeMus::zero(),
        true,
    );
    assert_eq!(geometry_source_end(&collapsed), 1);
}

fn model_for(slices: &[AudioSlice]) -> SliceModel<'_> {
    slice_model(slices, map(), TimeMus::new(16, 1), TimeMus::zero())
}

fn lower(slices: &[AudioSlice], plan: Plan<ClipLane, SliceId, AudioSlice>) -> SliceLowered {
    lower_slice_plan(slices, &map(), plan)
}

fn slice_selection(ids: &[u128]) -> Selection<ClipLane, SliceId> {
    Selection::objects(
        ids.iter()
            .map(|id| SliceId::from_uuid(Uuid::from_u128(*id))),
    )
}

fn time_selection(start: i64, end: i64) -> Selection<ClipLane, SliceId> {
    Selection::time(
        RangeMus::new(TimeMus::new(start, 1), TimeMus::new(end, 1)).unwrap(),
        [ClipLane],
    )
}

#[test]
fn node_copy_paste_preserves_offsets_and_assigns_new_ids() {
    let slices = vec![slice_at(1, 1, 0, 24_000), slice_at(2, 3, 24_000, 48_000)];
    let model = model_for(&slices);
    let extract = edit::copy(&model, &slice_selection(&[1, 2])).unwrap();
    assert_eq!(extract.mode, notwig_core::edit::SelectionMode::Objects);
    assert_eq!(extract.fragments[0].offset, TimeMus::zero());
    assert_eq!(extract.fragments[1].offset, TimeMus::new(2, 1));
    let plan = edit::paste(&model, &extract, TimeMus::new(5, 1)).unwrap();
    let lowered = lower(&slices, plan);
    assert_eq!(lowered.slices.len(), 4);
    let new_starts: Vec<_> = lowered
        .slices
        .iter()
        .filter(|slice| slice.start >= TimeMus::new(5, 1))
        .map(|slice| slice.start)
        .collect();
    assert_eq!(new_starts, vec![TimeMus::new(5, 1), TimeMus::new(7, 1)]);
    match lowered.selection {
        SliceSelection::Objects(objects) => assert_eq!(objects.len(), 2),
        _ => panic!("expected object selection"),
    }
}

#[test]
fn bar_copy_trims_source_and_offsets_to_range() {
    let slices = vec![slice_at(1, 1, 0, 96_000)];
    let model = model_for(&slices);
    let range = RangeMus::new(TimeMus::new(3, 2), TimeMus::new(7, 2)).unwrap();
    let extract = edit::copy(&model, &Selection::time(range, [ClipLane])).unwrap();
    assert_eq!(extract.mode, notwig_core::edit::SelectionMode::Time);
    assert_eq!(extract.length, TimeMus::new(2, 1));
    assert_eq!(extract.fragments.len(), 1);
    assert_eq!(extract.fragments[0].offset, TimeMus::zero());
    assert_eq!(extract.fragments[0].content.source_start_sample, 12_000);
    assert_eq!(extract.fragments[0].content.source_end_sample, 60_000);
}

#[test]
fn delete_range_splits_a_slice_in_the_middle() {
    let slices = vec![slice_at(1, 1, 0, 96_000)];
    let range = RangeMus::new(TimeMus::new(2, 1), TimeMus::new(3, 1)).unwrap();
    let updated = delete_range(&map(), &slices, range);
    assert_eq!(updated.len(), 2);
    assert_eq!(updated[0].start, TimeMus::new(1, 1));
    assert_eq!(updated[0].source_end_sample, 24_000);
    assert_eq!(updated[1].start, TimeMus::new(3, 1));
    assert_eq!(updated[1].source_start_sample, 48_000);
    assert_ne!(updated[0].id, updated[1].id);
}

#[test]
fn delete_range_trims_edges_and_removes_covered_slices() {
    let slices = vec![slice_at(1, 1, 0, 96_000)];
    let left = delete_range(
        &map(),
        &slices,
        RangeMus::new(TimeMus::new(0, 1), TimeMus::new(2, 1)).unwrap(),
    );
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].start, TimeMus::new(2, 1));
    assert_eq!(left[0].source_start_sample, 24_000);

    let right = delete_range(
        &map(),
        &slices,
        RangeMus::new(TimeMus::new(4, 1), TimeMus::new(8, 1)).unwrap(),
    );
    assert_eq!(right.len(), 1);
    assert_eq!(right[0].start, TimeMus::new(1, 1));
    assert_eq!(right[0].source_end_sample, 72_000);

    let covered = delete_range(
        &map(),
        &slices,
        RangeMus::new(TimeMus::zero(), TimeMus::new(8, 1)).unwrap(),
    );
    assert_eq!(covered.len(), 0);
}

#[test]
fn cut_range_copies_before_deleting() {
    let slices = vec![slice_at(1, 1, 0, 96_000)];
    let model = model_for(&slices);
    let selection = time_selection(2, 3);
    let (extract, plan) = edit::cut(&model, &selection).unwrap();
    let lowered = lower(&slices, plan);
    assert_eq!(extract.fragments.len(), 1);
    assert_eq!(extract.fragments[0].content.source_start_sample, 24_000);
    assert_eq!(lowered.slices.len(), 2);
}

#[test]
fn bar_paste_places_trimmed_content_at_the_cursor_and_selects_the_range() {
    let slices = vec![slice_at(1, 0, 0, 96_000)];
    let model = model_for(&slices);
    let extract = edit::copy(&model, &time_selection(1, 3)).unwrap();
    let plan = edit::paste(&model, &extract, TimeMus::new(5, 1)).unwrap();
    let lowered = lower(&slices, plan);
    match lowered.selection {
        SliceSelection::Time(range) => {
            assert_eq!(
                range,
                RangeMus::new(TimeMus::new(5, 1), TimeMus::new(7, 1)).unwrap()
            );
        }
        _ => panic!("expected time selection"),
    }
    let pasted = lowered
        .slices
        .iter()
        .find(|slice| slice.start == TimeMus::new(5, 1))
        .expect("pasted slice");
    assert_eq!(pasted.source_start_sample, 24_000);
}

#[test]
fn node_cut_copies_before_deleting() {
    let slices = vec![slice_at(1, 0, 0, 24_000), slice_at(2, 1, 0, 24_000)];
    let model = model_for(&slices);
    let (extract, plan) = edit::cut(&model, &slice_selection(&[1])).unwrap();
    let lowered = lower(&slices, plan);
    assert_eq!(extract.fragments.len(), 1);
    assert_eq!(lowered.slices.len(), 1);
    assert_eq!(lowered.slices[0].id, slices[1].id);
}

#[test]
fn delete_nodes_removes_only_selected_nodes() {
    let slices = vec![slice_at(1, 0, 0, 24_000), slice_at(2, 1, 0, 24_000)];
    let model = model_for(&slices);
    let plan = edit::delete(&model, &slice_selection(&[1])).unwrap();
    let lowered = lower(&slices, plan);
    assert_eq!(lowered.slices.len(), 1);
    assert_eq!(lowered.slices[0].id, slices[1].id);
}

#[test]
fn duplicate_nodes_places_clone_after_the_selection() {
    let slices = vec![slice_at(1, 1, 0, 24_000), slice_at(2, 3, 0, 24_000)];
    let model = model_for(&slices);
    let plan = edit::duplicate(&model, &slice_selection(&[1, 2])).unwrap();
    let lowered = lower(&slices, plan);
    let starts: Vec<_> = lowered.slices.iter().map(|slice| slice.start).collect();
    assert_eq!(
        starts,
        vec![
            TimeMus::new(1, 1),
            TimeMus::new(3, 1),
            TimeMus::new(4, 1),
            TimeMus::new(6, 1),
        ]
    );
    match lowered.selection {
        SliceSelection::Objects(objects) => assert_eq!(objects.len(), 2),
        _ => panic!("expected object selection"),
    }
}

#[test]
fn duplicate_range_places_clone_at_range_end_and_selects_it() {
    let slices = vec![slice_at(1, 0, 0, 96_000)];
    let model = model_for(&slices);
    let plan = edit::duplicate(&model, &time_selection(1, 3)).unwrap();
    let lowered = lower(&slices, plan);
    match lowered.selection {
        SliceSelection::Time(range) => {
            assert_eq!(
                range,
                RangeMus::new(TimeMus::new(3, 1), TimeMus::new(5, 1)).unwrap()
            );
        }
        _ => panic!("expected time selection"),
    }
    let pasted = lowered
        .slices
        .iter()
        .find(|slice| slice.start == TimeMus::new(3, 1))
        .expect("duplicated slice");
    assert_eq!(pasted.source_start_sample, 24_000);
}

#[test]
fn time_move_clips_and_leaves_outside_parts() {
    let slices = vec![slice_at(1, 1, 0, 48_000), slice_at(2, 5, 0, 24_000)];
    let model = model_for(&slices);
    let plan = edit::move_selection(&model, &time_selection(1, 2), TimeMus::new(2, 1)).unwrap();
    let lowered = lower(&slices, plan);
    let starts: Vec<_> = lowered.slices.iter().map(|slice| slice.start).collect();
    assert_eq!(
        starts,
        vec![TimeMus::new(2, 1), TimeMus::new(3, 1), TimeMus::new(5, 1),]
    );
    let remainder = lowered
        .slices
        .iter()
        .find(|slice| slice.start == TimeMus::new(2, 1))
        .unwrap();
    assert_eq!(remainder.source_start_sample, 24_000);
    assert_eq!(remainder.source_end_sample, 48_000);
    let moved = lowered
        .slices
        .iter()
        .find(|slice| slice.start == TimeMus::new(3, 1))
        .unwrap();
    assert_eq!(moved.source_start_sample, 0);
    assert_eq!(moved.source_end_sample, 24_000);
    match lowered.selection {
        SliceSelection::Time(range) => {
            assert_eq!(
                range,
                RangeMus::new(TimeMus::new(3, 1), TimeMus::new(4, 1)).unwrap()
            );
        }
        _ => panic!("expected time selection"),
    }
}

#[test]
fn algebra_trim_start_keeps_right_edge_and_advances_source() {
    let slices = vec![slice_at(1, 1, 0, 48_000)];
    let model = model_for(&slices);
    let plan =
        edit::trim_object(&model, slices[0].id, edit::Edge::Start, TimeMus::new(2, 1)).unwrap();
    let lowered = lower(&slices, plan);
    let slice = &lowered.slices[0];
    assert_eq!(slice.start, TimeMus::new(2, 1));
    assert_eq!(slice.source_start_sample, 24_000);
    assert_eq!(slice.source_end_sample, 48_000);
}

#[test]
fn algebra_trim_end_extends_source_and_clamps_non_empty() {
    let slices = vec![slice_at(1, 1, 0, 48_000)];
    let model = model_for(&slices);
    let plan =
        edit::trim_object(&model, slices[0].id, edit::Edge::End, TimeMus::new(4, 1)).unwrap();
    let lowered = lower(&slices, plan);
    assert_eq!(lowered.slices[0].source_end_sample, 72_000);

    let plan = edit::trim_object(&model, slices[0].id, edit::Edge::End, TimeMus::zero()).unwrap();
    let lowered = lower(&slices, plan);
    let slice = &lowered.slices[0];
    assert!(slice.start < slice.start + source_duration(&map(), slice));
    assert!(slice.source_end_sample > slice.source_start_sample);
}

#[test]
fn time_move_clamps_to_lane_start() {
    let slices = vec![slice_at(1, 1, 0, 48_000)];
    let model = model_for(&slices);
    let plan = edit::move_selection(&model, &time_selection(1, 2), TimeMus::new(-8, 1)).unwrap();
    match plan.next {
        NextSelection::Time(time) => {
            assert_eq!(
                time.span,
                RangeMus::new(TimeMus::zero(), TimeMus::new(1, 1)).unwrap()
            );
        }
        _ => panic!("expected time selection"),
    }
}

#[test]
fn events_can_move_before_the_clip_start() {
    let slices = vec![slice_at(1, 0, 0, 24_000)];
    let model = slice_model(&slices, map(), TimeMus::new(4, 1), TimeMus::new(2, 1));
    let plan = edit::move_selection(
        &model,
        &Selection::objects([slices[0].id]),
        TimeMus::new(-1, 1),
    )
    .unwrap();
    match &plan.ops[0] {
        EditOp::Move { span, .. } => assert_eq!(span.start, TimeMus::new(-1, 1)),
        _ => panic!("expected move"),
    }
}

#[test]
fn clip_edge_drag_resizes_and_clamps() {
    let origin = RangeMus::new(TimeMus::new(2, 1), TimeMus::new(6, 1)).unwrap();
    let start = edge_preview_range(
        clip_edge_drag(ClipEdge::Start, egui::pos2(0.0, 0.0), origin),
        egui::pos2(80.0, 0.0),
        80.0,
        TimeMus::new(1, 4),
        false,
    )
    .unwrap();
    assert_eq!(
        start,
        RangeMus::new(TimeMus::new(3, 1), TimeMus::new(6, 1)).unwrap()
    );

    let end = edge_preview_range(
        clip_edge_drag(ClipEdge::End, egui::pos2(0.0, 0.0), origin),
        egui::pos2(80.0, 0.0),
        80.0,
        TimeMus::new(1, 4),
        false,
    )
    .unwrap();
    assert_eq!(
        end,
        RangeMus::new(TimeMus::new(2, 1), TimeMus::new(7, 1)).unwrap()
    );

    let clamped = edge_preview_range(
        clip_edge_drag(ClipEdge::Start, egui::pos2(0.0, 0.0), origin),
        egui::pos2(10_000.0, 0.0),
        80.0,
        TimeMus::new(1, 4),
        false,
    )
    .unwrap();
    assert!(clamped.length() > TimeMus::zero());
    assert_eq!(clamped.end, TimeMus::new(6, 1));
}

#[test]
fn zoom_keeps_local_beat_under_pointer() {
    let mut state = ClipEditorState::new(ClipId::nil());
    let left = 100.0;
    let pointer = 420.0;
    let before = state.viewport.start.as_f64()
        + f64::from((pointer - left) / state.viewport.pixels_per_beat);
    state.viewport.zoom_at(pointer, left, 2.0);
    let after = state.viewport.start.as_f64()
        + f64::from((pointer - left) / state.viewport.pixels_per_beat);
    assert!((before - after).abs() < 1e-6);
}

#[test]
fn command_x_cuts_the_clip_editor_bar_selection() {
    let mut ui = NotwigUi::default();
    ui.edit_focus = EditFocus::ClipEditor;
    let map = map();
    let mut audio = audio();
    let clip = notwig_core::ClipEntity {
        id: ClipId::nil(),
        track_id: notwig_core::TrackId::nil(),
        kind: notwig_core::sequencer::ClipKind::Audio,
        range: RangeMus::new(TimeMus::zero(), TimeMus::new(4, 1)).unwrap(),
        name: None,
        color: None,
        muted: false,
        payload: ClipPayload::Audio(audio.clone()),
    };
    let mut state = ClipEditorState::new(clip.id);
    state.select_bar(RangeMus::new(TimeMus::new(1, 1), TimeMus::new(2, 1)).unwrap());
    let ctx = egui::Context::default();
    let _ = ctx.run(
        egui::RawInput {
            modifiers: egui::Modifiers {
                command: true,
                ..Default::default()
            },
            events: vec![egui::Event::Key {
                key: egui::Key::X,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    command: true,
                    ..Default::default()
                },
            }],
            ..Default::default()
        },
        |ctx| {
            handle_clip_editor_actions(
                &mut ui,
                ctx,
                &mut state,
                &mut audio,
                &clip,
                &map,
                TimeMus::new(2, 1),
            );
        },
    );
    let slices = ui
        .intents
        .iter()
        .find_map(|intent| match intent {
            UiIntent::SetClipSlices { slices, .. } => Some(slices.clone()),
            _ => None,
        })
        .expect("cut should emit slices");
    assert_eq!(slices.len(), 1);
    assert_eq!(slices[0].start, TimeMus::new(2, 1));
    assert_eq!(slices[0].source_start_sample, 24_000);
}

#[test]
fn deleting_last_slice_keeps_clip_in_slice_mode() {
    let mut ui = NotwigUi::default();
    let mut state = ClipEditorState::new(ClipId::nil());
    let mut audio = audio();
    emit_slices(&mut ui, &mut state, &mut audio, Vec::new());
    assert_eq!(audio.slices.len(), 0);
    assert!(audio.slices_enabled);
    assert!(ui.intents.iter().any(|intent| matches!(
        intent,
        UiIntent::SetClipSlices { slices, .. } if slices.is_empty()
    )));
}

#[test]
fn emit_keeps_events_outside_the_clip_range() {
    let mut ui = NotwigUi::default();
    let mut state = ClipEditorState::new(ClipId::nil());
    let mut audio = audio();
    let mut out = slice();
    out.start = TimeMus::new(5, 1);
    emit_slices(&mut ui, &mut state, &mut audio, vec![out]);
    assert!(
        !ui.intents
            .iter()
            .any(|intent| matches!(intent, UiIntent::EditClipRange { .. }))
    );
    assert!(
        ui.intents
            .iter()
            .any(|intent| matches!(intent, UiIntent::SetClipSlices { .. }))
    );
}
