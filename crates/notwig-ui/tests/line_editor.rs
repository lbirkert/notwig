use egui::{Event, Id, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};
use notwig_core::{Curve, CurvePoint, TimeMus};
use notwig_ui::test_api::line_editor::{LineEditorConfig, LineEditorState, show};
use notwig_ui::test_api::timeline::TimelineGrid;

const PPB: f32 = 80.0;
const WIDTH: f32 = 800.0;
const HEIGHT: f32 = 400.0;

fn grid() -> TimelineGrid {
    TimelineGrid {
        start: TimeMus::zero(),
        pixels_per_beat: PPB,
        bar_length: TimeMus::new(4, 1),
        beat_length: TimeMus::new(1, 1),
        step: TimeMus::new(1, 1),
        snap_enabled: true,
    }
}

fn config() -> LineEditorConfig {
    LineEditorConfig {
        value_min: 0.0,
        value_max: 1.0,
        baseline: Some(0.5),
        min_time: Some(TimeMus::zero()),
        max_time: Some(TimeMus::new(8, 1)),
        format_value: None,
    }
}

fn screen() -> Rect {
    Rect::from_min_size(Pos2::ZERO, Vec2::new(WIDTH, HEIGHT))
}

fn run(
    ctx: &egui::Context,
    curve: &mut Curve,
    state: &mut LineEditorState,
    events: Vec<Event>,
    modifiers: Modifiers,
) {
    run_at(ctx, curve, state, events, modifiers, None);
}

fn run_at(
    ctx: &egui::Context,
    curve: &mut Curve,
    state: &mut LineEditorState,
    events: Vec<Event>,
    modifiers: Modifiers,
    time: Option<f64>,
) {
    let _ = ctx.run(
        RawInput {
            screen_rect: Some(screen()),
            time,
            modifiers,
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    let _ = show(
                        ui,
                        screen(),
                        Id::new("line-editor-test"),
                        grid(),
                        &config(),
                        curve,
                        state,
                    );
                });
        },
    );
}

fn pointer(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::default(),
    }
}

fn beat(micro: i64) -> TimeMus {
    TimeMus::new(micro, 1)
}

fn point_at(time: i64, value: f64) -> CurvePoint {
    CurvePoint::new(beat(time), value)
}

fn point_curve() -> Curve {
    Curve::from_points([point_at(2, 0.2), point_at(6, 0.8)]).unwrap()
}

fn position(time: f64, value: f64) -> Pos2 {
    Pos2::new((time as f32) * PPB, HEIGHT - (value as f32) * HEIGHT)
}

#[test]
fn click_adds_a_snapped_point() {
    let ctx = egui::Context::default();
    let mut curve = Curve::new();
    let mut state = LineEditorState::default();
    run(&ctx, &mut curve, &mut state, Vec::new(), Modifiers::NONE);
    let pos = position(2.2, 0.5);
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![Event::PointerMoved(pos)],
        Modifiers::NONE,
    );
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![pointer(pos, true)],
        Modifiers::NONE,
    );
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![pointer(pos, false)],
        Modifiers::NONE,
    );
    assert_eq!(curve.points().len(), 1);
    assert_eq!(curve.points()[0].time, beat(2));
    assert!((curve.points()[0].value - 0.5).abs() < 1e-9);
    assert_eq!(state.selected.len(), 1);
}

#[test]
fn click_adds_a_second_point_at_the_same_time() {
    let ctx = egui::Context::default();
    let mut curve = point_curve();
    let mut state = LineEditorState::default();
    run(&ctx, &mut curve, &mut state, Vec::new(), Modifiers::NONE);
    let pos = position(2.0, 0.8);
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![Event::PointerMoved(pos)],
        Modifiers::NONE,
    );
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![pointer(pos, true)],
        Modifiers::NONE,
    );
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![pointer(pos, false)],
        Modifiers::NONE,
    );
    let points = curve.points_at(beat(2));
    assert_eq!(points.len(), 2);
    assert!((points[0].value - 0.2).abs() < 1e-6);
    assert!((points[1].value - 0.8).abs() < 1e-6);
    assert!((curve.value_at(beat(2), -1.0) - 0.8).abs() < 1e-6);
}

#[test]
fn dragging_a_point_moves_time_and_value() {
    let ctx = egui::Context::default();
    let mut curve = point_curve();
    let mut state = LineEditorState::default();
    run(&ctx, &mut curve, &mut state, Vec::new(), Modifiers::NONE);
    let start = position(2.0, 0.2);
    let end = position(3.1, 0.3);
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![Event::PointerMoved(start)],
        Modifiers::NONE,
    );
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![pointer(start, true)],
        Modifiers::NONE,
    );
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![Event::PointerMoved(end)],
        Modifiers::NONE,
    );
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![pointer(end, false)],
        Modifiers::NONE,
    );
    let moved = curve.points()[0];
    assert_eq!(moved.time, beat(3));
    assert!((moved.value - 0.3).abs() < 1e-9);
}

#[test]
fn shift_drag_uses_fine_time_and_value() {
    let ctx = egui::Context::default();
    let mut curve = point_curve();
    let mut state = LineEditorState::default();
    run(&ctx, &mut curve, &mut state, Vec::new(), Modifiers::NONE);
    let start = position(2.0, 0.2);
    let end = position(2.1, 0.3);
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![Event::PointerMoved(start)],
        Modifiers::SHIFT,
    );
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![pointer(start, true)],
        Modifiers::SHIFT,
    );
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![Event::PointerMoved(end)],
        Modifiers::SHIFT,
    );
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![pointer(end, false)],
        Modifiers::SHIFT,
    );
    let moved = curve.points()[0];
    assert!((moved.time.as_f64() - 2.1).abs() < 1e-6);
    assert!((moved.value - 0.2125).abs() < 1e-9);
}

#[test]
fn option_drag_bends_the_segment() {
    let ctx = egui::Context::default();
    let mut curve = point_curve();
    let mut state = LineEditorState::default();
    run(&ctx, &mut curve, &mut state, Vec::new(), Modifiers::NONE);
    let start = position(4.0, 0.5);
    let end = position(4.0, 0.625);
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![Event::PointerMoved(start)],
        Modifiers::ALT,
    );
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![pointer(start, true)],
        Modifiers::ALT,
    );
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![Event::PointerMoved(end)],
        Modifiers::ALT,
    );
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![pointer(end, false)],
        Modifiers::ALT,
    );
    assert!(curve.points()[0].curvature > 0.0);
    assert!(curve.points()[0].curvature <= 1.0);
}

#[test]
fn command_drag_moves_both_segment_points() {
    let ctx = egui::Context::default();
    let mut curve = point_curve();
    let mut state = LineEditorState::default();
    run(&ctx, &mut curve, &mut state, Vec::new(), Modifiers::NONE);
    let start = position(4.0, 0.5);
    let end = position(4.0, 0.625);
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![Event::PointerMoved(start)],
        Modifiers::COMMAND,
    );
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![pointer(start, true)],
        Modifiers::COMMAND,
    );
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![Event::PointerMoved(end)],
        Modifiers::COMMAND,
    );
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![pointer(end, false)],
        Modifiers::COMMAND,
    );
    assert!((curve.points()[0].value - 0.325).abs() < 1e-9);
    assert!((curve.points()[1].value - 0.925).abs() < 1e-9);
}

#[test]
fn marquee_selects_points_inside_the_rectangle() {
    let ctx = egui::Context::default();
    let mut curve =
        Curve::from_points([point_at(2, 0.2), point_at(4, 0.8), point_at(6, 0.5)]).unwrap();
    let mut state = LineEditorState::default();
    run(&ctx, &mut curve, &mut state, Vec::new(), Modifiers::NONE);
    let start = Pos2::new(1.0, HEIGHT - 1.0);
    let end = Pos2::new(4.2 * PPB, 1.0);
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![Event::PointerMoved(start)],
        Modifiers::NONE,
    );
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![pointer(start, true)],
        Modifiers::NONE,
    );
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![Event::PointerMoved(end)],
        Modifiers::NONE,
    );
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![pointer(end, false)],
        Modifiers::NONE,
    );
    assert!(state.selected.contains(&beat(2)));
    assert!(state.selected.contains(&beat(4)));
    assert!(!state.selected.contains(&beat(6)));
}

#[test]
fn dragging_a_point_stays_attached_across_frames() {
    let ctx = egui::Context::default();
    let mut curve = point_curve();
    let mut state = LineEditorState::default();
    run(&ctx, &mut curve, &mut state, Vec::new(), Modifiers::NONE);
    let start = position(2.0, 0.2);
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![Event::PointerMoved(start)],
        Modifiers::NONE,
    );
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![pointer(start, true)],
        Modifiers::NONE,
    );
    for step in 1..=4 {
        let end = position(2.0 + f64::from(step) * 0.25, 0.2 + f64::from(step) * 0.05);
        run(
            &ctx,
            &mut curve,
            &mut state,
            vec![Event::PointerMoved(end)],
            Modifiers::NONE,
        );
    }
    let end = position(3.0, 0.4);
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![pointer(end, false)],
        Modifiers::NONE,
    );
    assert_eq!(curve.points()[0].time, beat(3));
    assert!((curve.points()[0].value - 0.4).abs() < 1e-9);
}

#[test]
fn double_clicking_a_point_removes_it() {
    let ctx = egui::Context::default();
    let mut curve = point_curve();
    let mut state = LineEditorState::default();
    run_at(
        &ctx,
        &mut curve,
        &mut state,
        Vec::new(),
        Modifiers::NONE,
        Some(0.0),
    );
    let pos = position(2.0, 0.2);
    let mut t = 0.1;
    for _ in 0..2 {
        run_at(
            &ctx,
            &mut curve,
            &mut state,
            vec![Event::PointerMoved(pos)],
            Modifiers::NONE,
            Some(t),
        );
        t += 0.1;
        run_at(
            &ctx,
            &mut curve,
            &mut state,
            vec![pointer(pos, true)],
            Modifiers::NONE,
            Some(t),
        );
        t += 0.1;
        run_at(
            &ctx,
            &mut curve,
            &mut state,
            vec![pointer(pos, false)],
            Modifiers::NONE,
            Some(t),
        );
        t += 0.1;
    }
    assert_eq!(curve.points().len(), 1);
    assert_eq!(curve.points()[0].time, beat(6));
}

#[test]
fn clicking_the_line_adds_and_drags_a_point_immediately() {
    let ctx = egui::Context::default();
    let mut curve = point_curve();
    let mut state = LineEditorState::default();
    run(&ctx, &mut curve, &mut state, Vec::new(), Modifiers::NONE);
    let press = position(4.0, 0.5);
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![Event::PointerMoved(press)],
        Modifiers::NONE,
    );
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![pointer(press, true)],
        Modifiers::NONE,
    );
    assert_eq!(curve.points().len(), 3);
    assert_eq!(curve.points()[1].time, beat(4));

    let end = position(5.0, 0.6);
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![Event::PointerMoved(end)],
        Modifiers::NONE,
    );
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![pointer(end, false)],
        Modifiers::NONE,
    );
    let moved = curve
        .points()
        .iter()
        .find(|point| point.time == beat(5))
        .expect("the dragged point should land at beat 5");
    assert!(
        (moved.value - 0.6).abs() < 1e-6,
        "moved value {}",
        moved.value
    );
}

#[test]
fn release_applies_the_final_pointer_position() {
    let ctx = egui::Context::default();
    let mut curve = point_curve();
    let mut state = LineEditorState::default();
    run(&ctx, &mut curve, &mut state, Vec::new(), Modifiers::NONE);
    let start = position(2.0, 0.2);
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![Event::PointerMoved(start)],
        Modifiers::NONE,
    );
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![pointer(start, true)],
        Modifiers::NONE,
    );
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![Event::PointerMoved(position(3.0, 0.3))],
        Modifiers::NONE,
    );
    run(
        &ctx,
        &mut curve,
        &mut state,
        vec![pointer(position(4.0, 0.4), false)],
        Modifiers::NONE,
    );
    assert_eq!(curve.points()[0].time, beat(4));
    assert!((curve.points()[0].value - 0.4).abs() < 1e-6);
}
