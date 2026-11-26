use egui::{Pos2, Rect, Vec2};
use notwig_core::ids::{Asset, Clip, DeviceChain, Track};
use notwig_core::protocol::{Command, CommandPayload};
use notwig_core::sequencer::{AudioClipData, AudioSlice, ClipKind, ClipPayload, MidiClipData};
use notwig_core::tracks::TrackKind;
use notwig_core::{
    AssetEntry, AssetKind, Db, Id, Meta, ProjectDoc, RangeMus, SampleRate, SliceId, TimeMus,
    TrackId, apply,
};
use notwig_ui::{NotwigUi, OpenedEditor, RuntimeView, UiIntent};
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

fn empty_doc() -> ProjectDoc {
    let meta = Meta {
        name: "Arranger".into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: SampleRate::DEFAULT,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    };
    ProjectDoc::new_template(meta, id::<Track>(1), id::<DeviceChain>(100)).unwrap()
}

fn with_track(doc: &ProjectDoc, n: u128, kind: TrackKind, parent: Option<TrackId>) -> ProjectDoc {
    apply(
        doc,
        &Command::new(CommandPayload::CreateTrack {
            id: id::<Track>(n),
            chain_id: id::<DeviceChain>(n + 1000),
            kind,
            name: format!("Track {n}"),
            parent,
            index: None,
            color: None,
        }),
    )
    .unwrap()
    .doc
}

fn with_midi_clip(
    doc: &ProjectDoc,
    track: TrackId,
    n: u128,
    start: TimeMus,
    end: TimeMus,
) -> ProjectDoc {
    apply(
        doc,
        &Command::new(CommandPayload::CreateClip {
            id: id::<Clip>(n),
            track_id: track,
            kind: ClipKind::Midi,
            range: RangeMus::new(start, end).unwrap(),
            payload: ClipPayload::Midi(MidiClipData {
                length: end - start,
                content_start: TimeMus::zero(),
                notes: Vec::new(),
            }),
        }),
    )
    .unwrap()
    .doc
}

fn with_audio_clip(
    doc: &ProjectDoc,
    track: TrackId,
    n: u128,
    start: TimeMus,
    end: TimeMus,
) -> ProjectDoc {
    let asset = id::<Asset>(n + 2000);
    let doc = apply(
        doc,
        &Command::new(CommandPayload::RegisterAsset {
            asset_id: asset,
            entry: AssetEntry {
                id: asset,
                kind: AssetKind::Audio,
                uri: format!("assets/{n}.wav"),
                original_name: None,
                hash: None,
                format: "wav".into(),
                sample_rate: Some(48_000),
                channels: Some(1),
                duration_samples: Some(48_000),
                original_sample_rate: Some(48_000),
                original_channels: Some(1),
            },
        }),
    )
    .unwrap()
    .doc;
    apply(
        &doc,
        &Command::new(CommandPayload::CreateClip {
            id: id::<Clip>(n),
            track_id: track,
            kind: ClipKind::Audio,
            range: RangeMus::new(start, end).unwrap(),
            payload: ClipPayload::Audio(AudioClipData {
                asset_id: asset,
                source_start_sample: 0,
                source_end_sample: 48_000,
                gain_db: Db(0.0),
                gain_curve: None,
                pan_curve: None,
                fade_in_samples: 0,
                fade_out_samples: 0,
                fade_shape: notwig_core::sequencer::FadeShape::Linear,
                slices: vec![AudioSlice {
                    id: SliceId::from_uuid(Uuid::from_u128(n + 3000)),
                    start: TimeMus::zero(),
                    source_start_sample: 0,
                    source_end_sample: 48_000,
                    silent_prefix_samples: 0,
                    gain_db: Db(0.0),
                    asset_id: None,
                }],
                slices_enabled: true,
            }),
        }),
    )
    .unwrap()
    .doc
}

fn screen() -> Rect {
    Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 900.0))
}

#[allow(clippy::too_many_arguments)]
fn frame(
    ctx: &egui::Context,
    ui: &mut NotwigUi,
    doc: &ProjectDoc,
    runtime: &RuntimeView,
    time: f64,
    events: Vec<egui::Event>,
) -> (Vec<UiIntent>, egui::FullOutput) {
    let mut intents = Vec::new();
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen()),
            time: Some(time),
            events,
            ..Default::default()
        },
        |ctx| {
            intents = ui.show(ctx, doc, runtime);
        },
    );
    (intents, output)
}

fn mount(doc: &ProjectDoc) -> (NotwigUi, egui::Context, RuntimeView) {
    let mut ui = NotwigUi::default();
    let ctx = egui::Context::default();
    let runtime = RuntimeView::default();
    frame(&ctx, &mut ui, doc, &runtime, 0.0, Vec::new());
    (ui, ctx, runtime)
}

fn button(pos: Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::default(),
    }
}

fn secondary_button(pos: Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Secondary,
        pressed,
        modifiers: egui::Modifiers::default(),
    }
}

fn double_click(
    ui: &mut NotwigUi,
    ctx: &egui::Context,
    doc: &ProjectDoc,
    runtime: &RuntimeView,
    pos: Pos2,
    time: f64,
) -> Vec<UiIntent> {
    let mut intents = Vec::new();
    let (frame_intents, _) = frame(
        ctx,
        ui,
        doc,
        runtime,
        time,
        vec![egui::Event::PointerMoved(pos)],
    );
    intents.extend(frame_intents);
    let (frame_intents, _) = frame(ctx, ui, doc, runtime, time + 0.02, vec![button(pos, true)]);
    intents.extend(frame_intents);
    let (frame_intents, _) = frame(ctx, ui, doc, runtime, time + 0.04, vec![button(pos, false)]);
    intents.extend(frame_intents);
    let (frame_intents, _) = frame(ctx, ui, doc, runtime, time + 0.10, vec![button(pos, true)]);
    intents.extend(frame_intents);
    let (frame_intents, _) = frame(ctx, ui, doc, runtime, time + 0.12, vec![button(pos, false)]);
    intents.extend(frame_intents);
    intents
}

fn right_click(
    ui: &mut NotwigUi,
    ctx: &egui::Context,
    doc: &ProjectDoc,
    runtime: &RuntimeView,
    pos: Pos2,
    time: f64,
) {
    let _ = frame(
        ctx,
        ui,
        doc,
        runtime,
        time,
        vec![egui::Event::PointerMoved(pos)],
    );
    let _ = frame(
        ctx,
        ui,
        doc,
        runtime,
        time + 0.02,
        vec![secondary_button(pos, true)],
    );
    let _ = frame(
        ctx,
        ui,
        doc,
        runtime,
        time + 0.04,
        vec![secondary_button(pos, false)],
    );
}

#[derive(Debug, Clone, Copy)]
struct DropPreview {
    parent: Option<TrackId>,
    index: usize,
    depth: usize,
}

struct DragResult {
    preview: Option<DropPreview>,
    preview_output: egui::FullOutput,
    intents: Vec<UiIntent>,
}

fn drag_track(
    ui: &mut NotwigUi,
    ctx: &egui::Context,
    doc: &ProjectDoc,
    runtime: &RuntimeView,
    from: Pos2,
    to: Pos2,
    time: f64,
) -> DragResult {
    let _ = frame(
        ctx,
        ui,
        doc,
        runtime,
        time,
        vec![egui::Event::PointerMoved(from)],
    );
    let _ = frame(ctx, ui, doc, runtime, time + 0.02, vec![button(from, true)]);
    let _ = frame(
        ctx,
        ui,
        doc,
        runtime,
        time + 0.04,
        vec![egui::Event::PointerMoved(from + Vec2::new(0.0, 12.0))],
    );
    let (_, preview_output) = frame(
        ctx,
        ui,
        doc,
        runtime,
        time + 0.06,
        vec![egui::Event::PointerMoved(to)],
    );
    let preview = ui.track_drop.map(|drop| DropPreview {
        parent: drop.parent,
        index: drop.index,
        depth: drop.depth,
    });
    let (intents, _) = frame(ctx, ui, doc, runtime, time + 0.08, vec![button(to, false)]);
    DragResult {
        preview,
        preview_output,
        intents,
    }
}

fn x_for_beat(ui: &NotwigUi, beat: f64) -> f32 {
    ui.timeline_left
        + (beat - ui.state.viewport.start.as_f64()) as f32 * ui.state.viewport.pixels_per_beat
}

fn move_intent(intents: &[UiIntent]) -> Option<(Option<TrackId>, usize)> {
    intents.iter().find_map(|intent| match intent {
        UiIntent::MoveTrack { parent, index, .. } => Some((*parent, *index)),
        _ => None,
    })
}

fn white_horizontal_lines(output: &egui::FullOutput) -> Vec<[Pos2; 2]> {
    fn collect(shape: &egui::Shape, out: &mut Vec<[Pos2; 2]>) {
        match shape {
            egui::Shape::LineSegment { points, stroke }
                if stroke.color == egui::Color32::WHITE
                    && (points[0].y - points[1].y).abs() < 0.5 =>
            {
                out.push(*points);
            }
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut out);
    }
    out
}

fn white_vertical_lines(output: &egui::FullOutput) -> Vec<[Pos2; 2]> {
    fn collect(shape: &egui::Shape, out: &mut Vec<[Pos2; 2]>) {
        match shape {
            egui::Shape::LineSegment { points, stroke }
                if stroke.color == egui::Color32::WHITE
                    && (points[0].x - points[1].x).abs() < 0.5 =>
            {
                out.push(*points);
            }
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut out);
    }
    out
}

#[test]
fn zoom_keeps_pointer_beat_stable() {
    let mut ui = NotwigUi::default();
    ui.timeline_left = 230.0;
    ui.state.viewport.pixels_per_beat = 64.0;
    ui.state.viewport.start = TimeMus::zero();
    ui.state.viewport.zoom_at(430.0, ui.timeline_left, 2.0);
    assert!((ui.state.viewport.pixels_per_beat - 128.0).abs() < 1e-3);
    let beat = ui.state.viewport.start.as_f64()
        + f64::from((430.0 - ui.timeline_left) / ui.state.viewport.pixels_per_beat);
    assert!((beat - 3.125).abs() < 1e-6);
}

#[test]
fn panning_clamps_at_zero() {
    let mut ui = NotwigUi::default();
    ui.state.viewport.start = TimeMus::new(10, 1);
    ui.state.viewport.pan_beats(-4.0);
    assert_eq!(ui.state.viewport.start, TimeMus::new(6, 1));
    ui.state.viewport.pan_beats(-100.0);
    assert_eq!(ui.state.viewport.start, TimeMus::zero());
}

#[test]
fn double_click_inside_a_clip_does_not_create_a_clip() {
    let track = id::<Track>(2);
    let doc = with_midi_clip(
        &with_track(&empty_doc(), 2, TrackKind::Instrument, None),
        track,
        20,
        TimeMus::new(2, 1),
        TimeMus::new(6, 1),
    );
    let (mut ui, ctx, mut runtime) = mount(&doc);
    runtime.playhead = TimeMus::new(3, 1);
    let row = *ui.track_rows.get(&track).unwrap();
    let pos = Pos2::new(x_for_beat(&ui, 3.0), row.center().y);
    let intents = double_click(&mut ui, &ctx, &doc, &runtime, pos, 0.2);
    assert!(
        !intents
            .iter()
            .any(|intent| matches!(intent, UiIntent::CreateMidiClip { .. }))
    );
}

#[test]
fn double_click_truncates_the_end_before_the_next_clip() {
    let track = id::<Track>(2);
    let doc = with_midi_clip(
        &with_track(&empty_doc(), 2, TrackKind::Instrument, None),
        track,
        20,
        TimeMus::new(2, 1),
        TimeMus::new(6, 1),
    );
    let (mut ui, ctx, runtime) = mount(&doc);
    let row = *ui.track_rows.get(&track).unwrap();
    let pos = Pos2::new(x_for_beat(&ui, 0.0) + 2.0, row.center().y);
    let intents = double_click(&mut ui, &ctx, &doc, &runtime, pos, 0.2);
    let range = intents
        .iter()
        .find_map(|intent| match intent {
            UiIntent::CreateMidiClip { range, .. } => Some(*range),
            _ => None,
        })
        .expect("double click should create a clip");
    assert_eq!(range.start, TimeMus::zero());
    assert_eq!(range.end, TimeMus::new(2, 1));
}

#[test]
fn double_clicking_a_track_header_opens_the_device_editor() {
    let track = id::<Track>(2);
    let doc = with_track(&empty_doc(), 2, TrackKind::Instrument, None);
    let (mut ui, ctx, runtime) = mount(&doc);
    ui.currently_opened = None;
    let row = *ui.track_rows.get(&track).unwrap();
    let pos = Pos2::new(row.left() + 40.0, row.top() + 10.0);
    let _ = double_click(&mut ui, &ctx, &doc, &runtime, pos, 0.2);
    assert_eq!(ui.currently_opened, Some(OpenedEditor::DeviceEditor));
    assert_eq!(ui.state.selection.tracks.primary(), Some(track));
}

#[test]
fn right_dragging_a_track_header_does_not_select_or_move_it() {
    let first = id::<Track>(2);
    let second = id::<Track>(3);
    let doc = with_track(
        &with_track(&empty_doc(), 2, TrackKind::Instrument, None),
        3,
        TrackKind::Instrument,
        None,
    );
    let (mut ui, ctx, runtime) = mount(&doc);
    ui.state.selection.select_track(first, false);
    let row = *ui.track_rows.get(&second).unwrap();
    let from = Pos2::new(row.left() + 40.0, row.top() + 10.0);
    let to = Pos2::new(from.x, from.y + 20.0);
    let _ = frame(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        0.2,
        vec![egui::Event::PointerMoved(from)],
    );
    let _ = frame(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        0.22,
        vec![secondary_button(from, true)],
    );
    let _ = frame(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        0.24,
        vec![egui::Event::PointerMoved(to)],
    );
    let (intents, _) = frame(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        0.26,
        vec![secondary_button(to, false)],
    );
    assert_eq!(ui.state.selection.tracks.primary(), Some(first));
    assert!(ui.track_drag.is_none());
    assert!(move_intent(&intents).is_none());
}

#[test]
fn right_clicking_a_lane_selects_the_clip_under_the_cursor() {
    let track = id::<Track>(2);
    let clip = id::<Clip>(20);
    let doc = with_midi_clip(
        &with_track(&empty_doc(), 2, TrackKind::Instrument, None),
        track,
        20,
        TimeMus::zero(),
        TimeMus::new(4, 1),
    );
    let (mut ui, ctx, runtime) = mount(&doc);
    let row = *ui.track_rows.get(&track).unwrap();
    let pos = Pos2::new(x_for_beat(&ui, 1.0), row.center().y);
    right_click(&mut ui, &ctx, &doc, &runtime, pos, 0.2);
    assert!(ui.state.selection.clips.contains(&clip));
    assert_eq!(ui.state.selection.tracks.primary(), Some(track));
}

#[test]
fn no_op_track_insert_positions_are_hidden_and_drop_nothing() {
    let doc = with_track(
        &with_track(
            &with_track(&empty_doc(), 2, TrackKind::Instrument, None),
            3,
            TrackKind::Instrument,
            None,
        ),
        4,
        TrackKind::Instrument,
        None,
    );
    let dragged = id::<Track>(3);
    let next = id::<Track>(4);
    let (mut ui, ctx, runtime) = mount(&doc);
    let from_row = *ui.track_rows.get(&dragged).unwrap();
    let next_row = *ui.track_rows.get(&next).unwrap();
    let from = Pos2::new(from_row.left() + 40.0, from_row.top() + 10.0);
    let to = Pos2::new(from.x, next_row.top() + 2.0);
    let result = drag_track(&mut ui, &ctx, &doc, &runtime, from, to, 0.2);
    assert!(result.preview.is_none());
    assert!(move_intent(&result.intents).is_none());
    assert!(
        white_horizontal_lines(&result.preview_output)
            .iter()
            .all(|line| (line[0].y - to.y).abs() > 0.5)
    );
}

#[test]
fn dragging_a_track_down_emits_the_resulting_insert_index() {
    let first = id::<Track>(2);
    let next = id::<Track>(4);
    let doc = with_track(
        &with_track(
            &with_track(&empty_doc(), 2, TrackKind::Instrument, None),
            3,
            TrackKind::Instrument,
            None,
        ),
        4,
        TrackKind::Instrument,
        None,
    );
    let (mut ui, ctx, runtime) = mount(&doc);
    let from_row = *ui.track_rows.get(&first).unwrap();
    let next_row = *ui.track_rows.get(&next).unwrap();
    let from = Pos2::new(from_row.left() + 40.0, from_row.top() + 10.0);
    let to = Pos2::new(from.x, next_row.top() + 2.0);
    let result = drag_track(&mut ui, &ctx, &doc, &runtime, from, to, 0.2);
    let preview = result.preview.expect("insert position should be visible");
    assert_eq!(preview.parent, None);
    assert_eq!(preview.index, 2);
    assert_eq!(move_intent(&result.intents), Some((None, 2)));
}

#[test]
fn a_track_can_move_beyond_a_group_that_is_last() {
    let group = id::<Track>(2);
    let child = id::<Track>(3);
    let doc = with_track(
        &with_track(&empty_doc(), 2, TrackKind::Group, None),
        3,
        TrackKind::Instrument,
        Some(group),
    );
    let (mut ui, ctx, runtime) = mount(&doc);
    let child_row = *ui.track_rows.get(&child).unwrap();
    let from = Pos2::new(child_row.left() + 40.0, child_row.top() + 10.0);
    let to = Pos2::new(child_row.left() + 8.0, child_row.bottom() + 10.0);
    let result = drag_track(&mut ui, &ctx, &doc, &runtime, from, to, 0.2);
    let preview = result
        .preview
        .expect("root insert position should be visible");
    assert_eq!(preview.parent, None);
    assert_eq!(preview.index, 2);
    assert_eq!(move_intent(&result.intents), Some((None, 2)));
}

#[test]
fn group_insert_lines_are_indented() {
    let group = id::<Track>(2);
    let child = id::<Track>(3);
    let other = id::<Track>(4);
    let doc = with_track(
        &with_track(
            &with_track(&empty_doc(), 2, TrackKind::Group, None),
            3,
            TrackKind::Instrument,
            Some(group),
        ),
        4,
        TrackKind::Instrument,
        None,
    );
    let (mut ui, ctx, runtime) = mount(&doc);
    let from_row = *ui.track_rows.get(&other).unwrap();
    let child_row = *ui.track_rows.get(&child).unwrap();
    let from = Pos2::new(from_row.left() + 40.0, from_row.top() + 10.0);
    let to = Pos2::new(from.x, child_row.top() + 2.0);
    let result = drag_track(&mut ui, &ctx, &doc, &runtime, from, to, 0.2);
    let preview = result
        .preview
        .expect("group insert position should be visible");
    assert_eq!(preview.parent, Some(group));
    assert_eq!(preview.index, 0);
    assert_eq!(preview.depth, 1);
    let expected_x = from_row.left() + 4.0 + 14.0;
    let expected_y = ui
        .track_rows
        .get(&child)
        .map_or(child_row.top(), egui::Rect::top);
    assert!(
        white_horizontal_lines(&result.preview_output)
            .iter()
            .any(|line| {
                (line[0].y - expected_y).abs() < 0.5
                    && (line[0].x - expected_x).abs() < 0.5
                    && line[1].x > line[0].x
            })
    );
}

#[test]
fn arranger_playhead_is_hidden_when_transport_is_stopped() {
    let doc = empty_doc();
    let mut ui = NotwigUi::default();
    let ctx = egui::Context::default();
    let mut runtime = RuntimeView {
        playhead: TimeMus::new(2, 1),
        ..RuntimeView::default()
    };
    let _ = frame(&ctx, &mut ui, &doc, &runtime, 0.0, Vec::new());
    let x = x_for_beat(&ui, 2.0);
    let (_, stopped) = frame(&ctx, &mut ui, &doc, &runtime, 0.1, Vec::new());
    assert!(
        white_vertical_lines(&stopped)
            .iter()
            .all(|line| (line[0].x - x).abs() > 0.5)
    );
    runtime.playing = true;
    let (_, playing) = frame(&ctx, &mut ui, &doc, &runtime, 0.2, Vec::new());
    assert!(
        white_vertical_lines(&playing)
            .iter()
            .any(|line| (line[0].x - x).abs() < 0.5)
    );
}

#[test]
fn dragging_a_clip_body_starts_a_time_selection_without_moving_the_clip() {
    let track = id::<Track>(2);
    let doc = with_midi_clip(
        &with_track(&empty_doc(), 2, TrackKind::Instrument, None),
        track,
        20,
        TimeMus::new(2, 1),
        TimeMus::new(6, 1),
    );
    let (mut ui, ctx, runtime) = mount(&doc);
    let row = *ui.track_rows.get(&track).unwrap();
    let from = Pos2::new(x_for_beat(&ui, 3.0), row.center().y);
    let to = Pos2::new(x_for_beat(&ui, 4.0), row.center().y);
    let _ = frame(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        0.2,
        vec![egui::Event::PointerMoved(from)],
    );
    let _ = frame(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        0.22,
        vec![button(from, true)],
    );
    let _ = frame(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        0.24,
        vec![egui::Event::PointerMoved(to)],
    );
    let (intents, _) = frame(&ctx, &mut ui, &doc, &runtime, 0.26, vec![button(to, false)]);

    let time = ui
        .state
        .selection
        .time
        .as_ref()
        .expect("clip body drag should create a time selection");
    assert_eq!(
        time.range,
        RangeMus::new(TimeMus::new(3, 1), TimeMus::new(4, 1)).unwrap()
    );
    assert!(ui.state.selection.clips.is_empty());
    assert!(!intents.iter().any(|intent| matches!(
        intent,
        UiIntent::MoveClip { .. } | UiIntent::MoveClips { .. }
    )));
}

#[test]
fn leftward_trim_at_source_start_extends_the_audio_clip() {
    let track = id::<Track>(2);
    let doc = with_audio_clip(
        &with_track(&empty_doc(), 2, TrackKind::Audio, None),
        track,
        20,
        TimeMus::new(2, 1),
        TimeMus::new(4, 1),
    );
    let (mut ui, ctx, runtime) = mount(&doc);
    let row = *ui.track_rows.get(&track).unwrap();
    let from = Pos2::new(x_for_beat(&ui, 2.0), row.top() + 10.0);
    let to = Pos2::new(x_for_beat(&ui, 1.0), row.top() + 10.0);
    let _ = frame(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        0.2,
        vec![egui::Event::PointerMoved(from)],
    );
    let _ = frame(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        0.22,
        vec![button(from, true)],
    );
    let _ = frame(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        0.24,
        vec![egui::Event::PointerMoved(to)],
    );
    let (intents, _) = frame(&ctx, &mut ui, &doc, &runtime, 0.26, vec![button(to, false)]);

    let range = intents
        .iter()
        .find_map(|intent| match intent {
            UiIntent::SetClipRange { range, .. } => Some(*range),
            _ => None,
        })
        .expect("leftward audio trim should set the clip range");
    assert_eq!(range.start, TimeMus::new(1, 1));
    assert_eq!(range.end, TimeMus::new(4, 1));
    assert!(
        range.start < TimeMus::new(2, 1),
        "the trim should extend before the original source start"
    );
}
