use egui::{Color32, Pos2, Rect, Vec2};
use notwig_core::ids::{Clip, DeviceChain, Track};
use notwig_core::protocol::{Command, CommandPayload};
use notwig_core::sequencer::{ClipKind, ClipPayload, MidiClipData};
use notwig_core::tracks::TrackKind;
use notwig_core::{Id, Meta, ProjectDoc, RangeMus, SampleRate, TimeMus};
use notwig_ui::{NotwigUi, RuntimeView, UiIntent};
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

fn empty_doc() -> ProjectDoc {
    let meta = Meta {
        name: "Arranger layout".into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: SampleRate::DEFAULT,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    };
    ProjectDoc::new_template(meta, id::<Track>(1), id::<DeviceChain>(100)).unwrap()
}

fn with_track(doc: &ProjectDoc, n: u128, kind: TrackKind) -> ProjectDoc {
    notwig_core::apply(
        doc,
        &Command::new(CommandPayload::CreateTrack {
            id: id::<Track>(n),
            chain_id: id::<DeviceChain>(n + 1000),
            kind,
            name: format!("Track {n}"),
            parent: None,
            index: None,
            color: None,
        }),
    )
    .unwrap()
    .doc
}

fn with_child_track(
    doc: &ProjectDoc,
    n: u128,
    kind: TrackKind,
    parent: notwig_core::TrackId,
) -> ProjectDoc {
    notwig_core::apply(
        doc,
        &Command::new(CommandPayload::CreateTrack {
            id: id::<Track>(n),
            chain_id: id::<DeviceChain>(n + 1000),
            kind,
            name: format!("Track {n}"),
            parent: Some(parent),
            index: None,
            color: None,
        }),
    )
    .unwrap()
    .doc
}

fn with_midi_clip(
    doc: &ProjectDoc,
    track: notwig_core::TrackId,
    n: u128,
    start: TimeMus,
    end: TimeMus,
) -> ProjectDoc {
    notwig_core::apply(
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

fn screen(height: f32) -> Rect {
    Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, height))
}

#[allow(clippy::too_many_arguments)]
fn frame(
    ctx: &egui::Context,
    ui: &mut NotwigUi,
    doc: &ProjectDoc,
    runtime: &RuntimeView,
    height: f32,
    time: f64,
    modifiers: egui::Modifiers,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(screen(height)),
            time: Some(time),
            modifiers,
            events,
            ..Default::default()
        },
        |ctx| {
            let _ = ui.show(ctx, doc, runtime);
        },
    )
}

fn mount(
    doc: &ProjectDoc,
    height: f32,
) -> (NotwigUi, egui::Context, RuntimeView, egui::FullOutput) {
    let mut ui = NotwigUi::default();
    let ctx = egui::Context::default();
    let runtime = RuntimeView::default();
    let output = frame(
        &ctx,
        &mut ui,
        doc,
        &runtime,
        height,
        0.0,
        egui::Modifiers::default(),
        Vec::new(),
    );
    (ui, ctx, runtime, output)
}

fn button(pos: Pos2, pressed: bool, shift: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers {
            shift,
            ..Default::default()
        },
    }
}

fn x_for_beat(ui: &NotwigUi, beat: f64) -> f32 {
    ui.timeline_left
        + (beat - ui.state.viewport.start.as_f64()) as f32 * ui.state.viewport.pixels_per_beat
}

fn horizontal_lines(output: &egui::FullOutput, color: Color32) -> Vec<[Pos2; 2]> {
    fn collect(shape: &egui::Shape, color: Color32, out: &mut Vec<[Pos2; 2]>) {
        match shape {
            egui::Shape::LineSegment { points, stroke }
                if stroke.color == color && (points[0].y - points[1].y).abs() < 0.5 =>
            {
                out.push(*points);
            }
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, color, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, color, &mut out);
    }
    out
}

fn white_vertical_lines(output: &egui::FullOutput) -> Vec<[Pos2; 2]> {
    fn collect(shape: &egui::Shape, out: &mut Vec<[Pos2; 2]>) {
        match shape {
            egui::Shape::LineSegment { points, stroke }
                if stroke.color == Color32::WHITE && (points[0].x - points[1].x).abs() < 0.5 =>
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
fn master_lane_is_pinned_to_the_bottom_of_the_arranger() {
    let doc = with_track(&empty_doc(), 2, TrackKind::Instrument);
    let master = doc.master_track().expect("master track");
    let (ui, _, _, _) = mount(&doc, 900.0);
    let master_rect = *ui.track_rows.get(&master).expect("master row");
    let track_rect = *ui.track_rows.get(&id::<Track>(2)).expect("track row");
    assert!(
        master_rect.top() >= track_rect.bottom(),
        "master should stay below the scrolling lane list"
    );
    assert!(
        (ui.arranger_rect.bottom() - master_rect.bottom()).abs() < 40.0,
        "master should sit at the bottom of the arranger"
    );
}

#[test]
fn horizontal_lane_dividers_follow_each_lane_boundary() {
    let track = id::<Track>(2);
    let doc = with_track(&empty_doc(), 2, TrackKind::Instrument);
    let (ui, _, _, output) = mount(&doc, 900.0);
    let row = *ui.track_rows.get(&track).expect("track row");
    let lines = horizontal_lines(&output, Color32::from_rgb(44, 47, 56));
    assert!(
        lines.iter().any(|line| {
            (line[0].y - row.bottom()).abs() < 0.5
                && (line[1].y - row.bottom()).abs() < 0.5
                && (line[1].x - line[0].x).abs() > 200.0
        }),
        "each lane should end with a subtle horizontal divider"
    );
}

#[test]
fn time_selection_markers_stop_at_unselected_lanes() {
    let first = id::<Track>(2);
    let second = id::<Track>(3);
    let doc = with_track(
        &with_track(&empty_doc(), 2, TrackKind::Instrument),
        3,
        TrackKind::Instrument,
    );
    let (mut ui, ctx, runtime, _) = mount(&doc, 900.0);
    ui.state.selection.select_time(
        RangeMus::new(TimeMus::new(1, 1), TimeMus::new(3, 1)).unwrap(),
        &[first],
    );
    let output = frame(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        900.0,
        0.1,
        egui::Modifiers::default(),
        Vec::new(),
    );
    let first_row = *ui.track_rows.get(&first).expect("first row");
    let second_row = *ui.track_rows.get(&second).expect("second row");
    let x = x_for_beat(&ui, 1.0);
    let marker_lines: Vec<_> = white_vertical_lines(&output)
        .into_iter()
        .filter(|line| (line[0].x - x).abs() < 0.5)
        .collect();
    assert!(!marker_lines.is_empty(), "selection marker should render");
    assert!(
        marker_lines
            .iter()
            .all(|line| line[0].max(line[1]).y <= first_row.bottom() + 0.5),
        "selection markers should only cover selected lanes"
    );
    assert!(
        marker_lines
            .iter()
            .any(|line| (line[0].max(line[1]).y - first_row.bottom()).abs() < 0.5),
        "selection markers should cover the selected lane"
    );
    assert!(
        second_row.top() >= first_row.bottom() - 0.5,
        "second lane should sit below the selected lane"
    );
}

#[test]
fn time_selection_start_edge_can_be_dragged_horizontally() {
    let track = id::<Track>(2);
    let doc = with_track(&empty_doc(), 2, TrackKind::Instrument);
    let (mut ui, ctx, runtime, _) = mount(&doc, 900.0);
    let row = *ui.track_rows.get(&track).expect("track row");
    ui.state.selection.select_time(
        RangeMus::new(TimeMus::new(2, 1), TimeMus::new(4, 1)).unwrap(),
        &[track],
    );
    let start = Pos2::new(x_for_beat(&ui, 2.0), row.center().y);
    let end = Pos2::new(x_for_beat(&ui, 1.0), row.center().y);
    let _ = frame(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        900.0,
        0.1,
        egui::Modifiers::default(),
        vec![egui::Event::PointerMoved(start)],
    );
    let _ = frame(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        900.0,
        0.12,
        egui::Modifiers::default(),
        vec![button(start, true, false)],
    );
    let _ = frame(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        900.0,
        0.14,
        egui::Modifiers::default(),
        vec![egui::Event::PointerMoved(end)],
    );
    let _ = frame(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        900.0,
        0.16,
        egui::Modifiers::default(),
        vec![button(end, false, false)],
    );
    let time = ui.state.selection.time.as_ref().expect("time selection");
    assert_eq!(time.range.start, TimeMus::new(1, 1));
    assert_eq!(time.range.end, TimeMus::new(4, 1));
}

#[test]
fn shift_dragging_a_clip_bypasses_grid_snapping() {
    let track = id::<Track>(2);
    let clip = id::<Clip>(20);
    let doc = with_midi_clip(
        &with_track(&empty_doc(), 2, TrackKind::Instrument),
        track,
        20,
        TimeMus::zero(),
        TimeMus::new(4, 1),
    );
    let (mut ui, ctx, runtime, _) = mount(&doc, 900.0);
    let row = *ui.track_rows.get(&track).expect("track row");
    let start = Pos2::new(x_for_beat(&ui, 1.0), row.top() + 12.0);
    let ppb = ui.state.viewport.pixels_per_beat;
    let end = Pos2::new(start.x + ppb * 0.37, start.y);
    let shift = egui::Modifiers {
        shift: true,
        ..Default::default()
    };
    let _ = frame(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        900.0,
        0.1,
        shift,
        vec![egui::Event::PointerMoved(start)],
    );
    let _ = frame(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        900.0,
        0.12,
        shift,
        vec![button(start, true, true)],
    );
    let _ = frame(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        900.0,
        0.14,
        shift,
        vec![egui::Event::PointerMoved(start + Vec2::new(0.0, 8.0))],
    );
    let _ = frame(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        900.0,
        0.16,
        shift,
        vec![egui::Event::PointerMoved(end)],
    );
    let mut intents = Vec::new();
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen(900.0)),
            time: Some(0.18),
            modifiers: shift,
            events: vec![button(end, false, true)],
            ..Default::default()
        },
        |ctx| {
            intents = ui.show(ctx, &doc, &runtime);
        },
    );
    let _ = output;
    let range = intents
        .iter()
        .find_map(|intent| match intent {
            UiIntent::MoveClip { clip_id, range, .. } if *clip_id == clip => Some(*range),
            _ => None,
        })
        .expect("shift drag should move the clip");
    let expected = TimeMus::new(370_000, 1_000_000);
    assert!(
        (range.start.as_f64() - expected.as_f64()).abs() < 1e-6,
        "shift should keep the unsnapped start, got {}",
        range.start.as_f64()
    );
}

#[test]
fn group_twisty_still_collapses_lanes() {
    let group = id::<Track>(2);
    let doc = with_child_track(
        &with_track(&empty_doc(), 2, TrackKind::Group),
        3,
        TrackKind::Instrument,
        group,
    );
    let (mut ui, ctx, runtime, _) = mount(&doc, 900.0);
    let row = *ui.track_rows.get(&group).expect("group row");
    let pos = Pos2::new(row.left() + 16.0, row.top() + 13.0);
    let _ = frame(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        900.0,
        0.1,
        egui::Modifiers::default(),
        vec![egui::Event::PointerMoved(pos)],
    );
    let _ = frame(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        900.0,
        0.12,
        egui::Modifiers::default(),
        vec![button(pos, true, false)],
    );
    let _ = frame(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        900.0,
        0.14,
        egui::Modifiers::default(),
        vec![button(pos, false, false)],
    );
    assert!(
        ui.collapsed_tracks.contains(&group),
        "clicking the twisty should still collapse the group"
    );
}

#[test]
fn shift_clicking_the_lane_drag_handle_keeps_multi_selection() {
    let first = id::<Track>(2);
    let second = id::<Track>(3);
    let doc = with_track(
        &with_track(&empty_doc(), 2, TrackKind::Instrument),
        3,
        TrackKind::Instrument,
    );
    let (mut ui, ctx, runtime, _) = mount(&doc, 900.0);
    ui.state.selection.select_track(first, false);
    let row = *ui.track_rows.get(&second).expect("second row");
    let pos = Pos2::new(row.left() + 40.0, row.top() + 10.0);
    let shift = egui::Modifiers {
        shift: true,
        ..Default::default()
    };
    let _ = frame(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        900.0,
        0.1,
        shift,
        vec![egui::Event::PointerMoved(pos)],
    );
    let _ = frame(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        900.0,
        0.12,
        shift,
        vec![button(pos, true, true)],
    );
    let _ = frame(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        900.0,
        0.14,
        shift,
        vec![button(pos, false, true)],
    );
    assert!(ui.state.selection.tracks.contains(&first));
    assert!(ui.state.selection.tracks.contains(&second));
    assert!(ui.track_drag.is_none(), "shift click must not start a drag");
}
