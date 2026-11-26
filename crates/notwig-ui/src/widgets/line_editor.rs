//! Reusable time/value line editor for automation and clip modulation.

use std::collections::BTreeSet;

use egui::{Color32, Id, Pos2, Rect, Sense, Stroke, Vec2};
use notwig_core::{Curve, CurvePoint, MAX_CURVATURE, MIN_CURVATURE, TimeMus};

use crate::theme;
use crate::timeline::TimelineGrid;
use crate::widgets::beats_to_time;

const POINT_RADIUS: f32 = 4.5;
const POINT_HIT_RADIUS: f32 = 9.0;
const LINE_HIT_RADIUS: f32 = 7.0;
const TIME_EPSILON: f64 = 1e-6;
const CURVE_SAMPLES: usize = 20;
const FINE_RATIO: f64 = 8.0;

/// Layout and value mapping for a line editor.
#[derive(Debug, Clone, Copy)]
pub struct LineEditorConfig {
    /// Lowest point value.
    pub value_min: f64,
    /// Highest point value.
    pub value_max: f64,
    /// Optional reference value drawn as a horizontal guide.
    pub baseline: Option<f64>,
    /// Optional lower time bound for point edits.
    pub min_time: Option<TimeMus>,
    /// Optional upper time bound for point edits.
    pub max_time: Option<TimeMus>,
    /// Formats a value for hover text.
    pub format_value: Option<fn(f64) -> String>,
}

impl Default for LineEditorConfig {
    fn default() -> Self {
        Self {
            value_min: 0.0,
            value_max: 1.0,
            baseline: None,
            min_time: None,
            max_time: None,
            format_value: None,
        }
    }
}

/// Persistent selection and gesture state for a line editor.
#[derive(Debug, Clone, Default)]
pub struct LineEditorState {
    pub selected: BTreeSet<TimeMus>,
    drag: Option<PointDrag>,
    segment: Option<SegmentDrag>,
    marquee: Option<MarqueeDrag>,
}

#[doc(hidden)]
pub mod test_api {
    pub use super::{LineEditorConfig, LineEditorResponse, LineEditorState, show};
}

impl LineEditorState {
    /// Returns whether points are selected.
    #[must_use]
    pub fn has_selection(&self) -> bool {
        !self.selected.is_empty()
    }

    /// Cancels in-progress point, segment, and marquee gestures.
    pub fn cancel_gestures(&mut self) {
        self.drag = None;
        self.segment = None;
        self.marquee = None;
    }
}

/// Result of one line-editor frame.
#[derive(Debug, Clone, Copy, Default)]
pub struct LineEditorResponse {
    /// The curve changed this frame.
    pub changed: bool,
    /// A gesture ended and the caller should commit one edit.
    pub committed: bool,
    /// The selection changed this frame.
    pub selection_changed: bool,
}

#[derive(Debug, Clone)]
struct PointDrag {
    origin_pointer: Pos2,
    origin_curve: Curve,
    selected: BTreeSet<TimeMus>,
    primary: TimeMus,
    changed: bool,
}

#[derive(Debug, Clone)]
struct SegmentDrag {
    origin_pointer: Pos2,
    origin_curve: Curve,
    left: TimeMus,
    mode: SegmentMode,
    changed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SegmentMode {
    Curvature,
    Move,
}

#[derive(Debug, Clone, Copy)]
struct MarqueeDrag {
    origin: Pos2,
    current: Pos2,
}

/// Shows a reusable line editor over `curve`.
pub fn show(
    ui: &mut egui::Ui,
    rect: Rect,
    id: Id,
    timeline: TimelineGrid,
    config: &LineEditorConfig,
    curve: &mut Curve,
    state: &mut LineEditorState,
) -> LineEditorResponse {
    if rect.width() <= 1.0 || rect.height() <= 1.0 {
        return LineEditorResponse::default();
    }
    if state.drag.is_none() && state.segment.is_none() && state.marquee.is_none() {
        state.selected.retain(|time| curve.point(*time).is_some());
    }
    let modifiers = ui.input(|input| input.modifiers);
    let bypass = modifiers.shift;
    let mut response = LineEditorResponse::default();

    let background = ui.interact(rect, id.with("line-editor-bg"), Sense::click_and_drag());
    let (primary_pressed, primary_released, press_origin) = ui.input(|input| {
        (
            input.pointer.primary_pressed(),
            input.pointer.primary_released(),
            input.pointer.press_origin(),
        )
    });
    let gesture_origin = press_origin
        .or_else(|| background.interact_pointer_pos())
        .or_else(|| ui.input(|input| input.pointer.hover_pos()));
    let point_at_origin =
        gesture_origin.and_then(|pointer| point_at(curve, timeline, rect, config, pointer));
    let pointer_over_point = point_at_origin.is_some();

    // Clicking the line inserts a point immediately and starts a point drag.
    if primary_pressed
        && !pointer_over_point
        && !modifiers.alt
        && !modifiers.command
        && let Some(pointer) = gesture_origin
        && line_hit(curve, timeline, rect, config, pointer)
    {
        let time = clamp_time(
            timeline.snap(time_for_x(timeline, rect, pointer.x), bypass),
            config,
        );
        let value = curve.value_at(time, value_for_y(config, rect, pointer.y));
        if let Some(point) = add_point(curve, time, value) {
            state.selected.clear();
            state.selected.insert(point);
            state.drag = Some(PointDrag {
                origin_pointer: pointer,
                origin_curve: curve.clone(),
                selected: state.selected.clone(),
                primary: point,
                changed: true,
            });
            response.changed = true;
            response.selection_changed = true;
        }
    }

    // Clicking empty space still adds a point on release.
    if background.clicked()
        && !pointer_over_point
        && state.drag.is_none()
        && !modifiers.alt
        && !modifiers.command
    {
        let time = clamp_time(
            timeline.snap(
                time_for_x(timeline, rect, pointer_x(&background, ui, rect)),
                bypass,
            ),
            config,
        );
        let value = value_for_y(config, rect, pointer_y(&background, ui, rect));
        if let Some(point) = add_point(curve, time, value) {
            state.selected.clear();
            state.selected.insert(point);
            response.changed = true;
            response.committed = true;
            response.selection_changed = true;
        }
    }

    if background.drag_started()
        && let Some(pointer) = drag_start_pos(ui, &background)
    {
        if state.drag.is_some() {
            // A line click already started the new point drag.
        } else if let Some(time) = point_at_origin {
            let additive = modifiers.command || modifiers.shift;
            if !state.selected.contains(&time) {
                if additive {
                    state.selected.insert(time);
                } else {
                    state.selected.clear();
                    state.selected.insert(time);
                }
                response.selection_changed = true;
            }
            state.drag = Some(PointDrag {
                origin_pointer: pointer,
                origin_curve: curve.clone(),
                selected: state.selected.clone(),
                primary: time,
                changed: false,
            });
        } else if (modifiers.alt || modifiers.command)
            && let Some(left) = segment_left_at(curve, timeline, rect, pointer.x)
        {
            state.segment = Some(SegmentDrag {
                origin_pointer: pointer,
                origin_curve: curve.clone(),
                left,
                mode: if modifiers.alt {
                    SegmentMode::Curvature
                } else {
                    SegmentMode::Move
                },
                changed: false,
            });
        } else {
            state.marquee = Some(MarqueeDrag {
                origin: pointer,
                current: pointer,
            });
        }
    }

    if background.dragged()
        && let Some(pointer) = background.interact_pointer_pos()
    {
        if let Some(drag) = state.drag.as_mut() {
            response.changed |= apply_point_drag(
                drag,
                pointer,
                timeline,
                rect,
                config,
                modifiers.shift,
                curve,
                &mut state.selected,
            );
        }
        if let Some(marquee) = &mut state.marquee {
            marquee.current = pointer;
            let selection = marquee_selection(curve, timeline, rect, config, marquee);
            if selection != state.selected {
                state.selected = selection;
                response.selection_changed = true;
            }
        }
        if let Some(segment) = &mut state.segment {
            response.changed |=
                apply_segment_drag(segment, pointer, rect, config, modifiers.shift, curve);
        }
    }

    if background.drag_stopped() {
        if let Some(marquee) = state.marquee.take() {
            let selection = marquee_selection(curve, timeline, rect, config, &marquee);
            if selection != state.selected {
                state.selected = selection;
                response.selection_changed = true;
            }
        }
        if let Some(segment) = state.segment.take() {
            response.changed |= segment.changed;
            response.committed |= segment.changed;
        }
    }
    if primary_released && let Some(drag) = state.drag.take() {
        response.changed |= drag.changed;
        response.committed |= drag.changed;
    }

    let points = curve.points().to_vec();
    for (index, point) in points.iter().enumerate() {
        let center = egui::pos2(
            x_for_time(timeline, rect, point.time),
            y_for_value(config, rect, point.value),
        );
        let hit = Rect::from_center_size(center, Vec2::splat(POINT_HIT_RADIUS * 2.0));
        let point_id = id.with(("point", index));
        let point_response = ui.interact(hit, point_id, Sense::click());
        if point_response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
        if point_response.double_clicked() {
            let points: Vec<CurvePoint> = curve
                .points()
                .iter()
                .enumerate()
                .filter(|(candidate, _)| *candidate != index)
                .map(|(_, point)| *point)
                .collect();
            if let Ok(next) = Curve::from_points(points) {
                *curve = next;
                if curve.point(point.time).is_none() {
                    state.selected.remove(&point.time);
                }
                response.changed = true;
                response.committed = true;
                response.selection_changed = true;
            }
            continue;
        }
        if point_response.clicked() {
            let additive = modifiers.command || modifiers.shift;
            if additive {
                if !state.selected.remove(&point.time) {
                    state.selected.insert(point.time);
                }
            } else if !state.selected.contains(&point.time) {
                state.selected.clear();
                state.selected.insert(point.time);
            }
            response.selection_changed = true;
        }
        if let Some(format) = config.format_value {
            let _ = point_response.on_hover_text(format(point.value));
        }
    }

    draw_curve(ui, rect, timeline, config, curve, state);
    response
}

fn draw_curve(
    ui: &mut egui::Ui,
    rect: Rect,
    timeline: TimelineGrid,
    config: &LineEditorConfig,
    curve: &Curve,
    state: &LineEditorState,
) {
    let painter = ui.painter().with_clip_rect(rect);
    if let Some(baseline) = config.baseline {
        let y = y_for_value(config, rect, baseline);
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            Stroke::new(1.0_f32, theme::GRID),
        );
    }

    let mut line = Vec::new();
    sample_curve(curve, timeline, rect, config, &mut line);
    for pair in line.windows(2) {
        let [a, b] = pair else {
            continue;
        };
        painter.add(egui::Shape::convex_polygon(
            vec![
                *a,
                *b,
                egui::pos2(b.x, rect.bottom()),
                egui::pos2(a.x, rect.bottom()),
            ],
            Color32::from_rgba_unmultiplied(255, 154, 46, 20),
            Stroke::NONE,
        ));
    }
    painter.add(egui::Shape::line(
        line,
        Stroke::new(2.0_f32, Color32::from_rgb(92, 72, 29)),
    ));
    for point in curve.points() {
        let center = egui::pos2(
            x_for_time(timeline, rect, point.time),
            y_for_value(config, rect, point.value),
        );
        let selected = state.selected.contains(&point.time);
        painter.circle(
            center,
            POINT_RADIUS,
            if selected {
                theme::ACCENT
            } else {
                Color32::from_rgb(214, 168, 58)
            },
            Stroke::new(
                1.5_f32,
                if selected {
                    Color32::WHITE
                } else {
                    Color32::from_rgb(70, 52, 18)
                },
            ),
        );
    }
    if let Some(marquee) = state.marquee {
        let marquee = Rect::from_two_pos(marquee.origin, marquee.current).intersect(rect);
        painter.rect_filled(
            marquee,
            egui::CornerRadius::ZERO,
            Color32::from_rgba_unmultiplied(255, 210, 120, 28),
        );
        painter.rect_stroke(
            marquee,
            egui::CornerRadius::ZERO,
            Stroke::new(1.0_f32, theme::ACCENT),
            egui::StrokeKind::Inside,
        );
    }
}

fn sample_curve(
    curve: &Curve,
    timeline: TimelineGrid,
    rect: Rect,
    config: &LineEditorConfig,
    out: &mut Vec<Pos2>,
) {
    let points = curve.points();
    if points.is_empty() {
        return;
    }
    let first = points[0];
    let last = points[points.len() - 1];
    out.push(egui::pos2(
        rect.left(),
        y_for_value(config, rect, first.value),
    ));
    for pair in points.windows(2) {
        let [left, right] = pair else {
            continue;
        };
        if left.time == right.time {
            // Vertical step: keep both endpoints so the line visibly jumps.
            let x = x_for_time(timeline, rect, left.time);
            out.push(egui::pos2(x, y_for_value(config, rect, left.value)));
            out.push(egui::pos2(x, y_for_value(config, rect, right.value)));
            continue;
        }
        for step in 0..=CURVE_SAMPLES {
            let ratio = step as f64 / CURVE_SAMPLES as f64;
            let time = TimeMus::new(
                ((left.time.as_f64() + (right.time.as_f64() - left.time.as_f64()) * ratio)
                    * 1_000_000.0)
                    .round() as i64,
                1_000_000,
            );
            let x = x_for_time(timeline, rect, time);
            let value = curve.value_at(time, left.value);
            out.push(egui::pos2(x, y_for_value(config, rect, value)));
        }
    }
    out.push(egui::pos2(
        rect.right(),
        y_for_value(config, rect, last.value),
    ));
    out.dedup_by(|a, b| a.distance(*b) < 0.01);
}

fn apply_point_drag(
    drag: &mut PointDrag,
    pointer: Pos2,
    timeline: TimelineGrid,
    rect: Rect,
    config: &LineEditorConfig,
    fine: bool,
    curve: &mut Curve,
    selected: &mut BTreeSet<TimeMus>,
) -> bool {
    let beats = f64::from(pointer.x - drag.origin_pointer.x) / f64::from(timeline.pixels_per_beat);
    let primary_raw = beats_to_time(drag.primary.as_f64() + beats);
    let primary_time = if fine {
        primary_raw
    } else {
        timeline.snap(primary_raw, false)
    };
    let mut delta_beats = primary_time.as_f64() - drag.primary.as_f64();
    let (min_delta, max_delta) = allowed_time_delta(&drag.origin_curve, &drag.selected);
    delta_beats = delta_beats.clamp(min_delta, max_delta);
    let mut delta_value = -f64::from(pointer.y - drag.origin_pointer.y) / f64::from(rect.height())
        * value_span(config);
    if fine {
        delta_value /= FINE_RATIO;
    }

    let mut points = Vec::with_capacity(drag.origin_curve.len());
    let mut next_selected = BTreeSet::new();
    for point in drag.origin_curve.points() {
        if drag.selected.contains(&point.time) {
            let mut moved = *point;
            moved.time = clamp_time(beats_to_time(point.time.as_f64() + delta_beats), config);
            moved.value = (point.value + delta_value).clamp(config.value_min, config.value_max);
            next_selected.insert(moved.time);
            points.push(moved);
        } else {
            points.push(*point);
        }
    }
    let Ok(next) = Curve::from_points(points) else {
        return false;
    };
    if next == *curve {
        return false;
    }
    *curve = next;
    *selected = next_selected;
    drag.changed = true;
    true
}

fn apply_segment_drag(
    drag: &mut SegmentDrag,
    pointer: Pos2,
    rect: Rect,
    config: &LineEditorConfig,
    fine: bool,
    curve: &mut Curve,
) -> bool {
    let mut points = drag.origin_curve.points().to_vec();
    let Some(left) = points.iter().position(|point| point.time == drag.left) else {
        return false;
    };
    let Some(right) = left.checked_add(1).filter(|index| *index < points.len()) else {
        return false;
    };
    let vertical = f64::from(drag.origin_pointer.y - pointer.y) / f64::from(rect.height());
    match drag.mode {
        SegmentMode::Curvature => {
            let mut delta = vertical * 4.0;
            if fine {
                delta /= FINE_RATIO;
            }
            points[left].curvature =
                (points[left].curvature + delta).clamp(MIN_CURVATURE, MAX_CURVATURE);
        }
        SegmentMode::Move => {
            let mut delta = vertical * value_span(config);
            if fine {
                delta /= FINE_RATIO;
            }
            points[left].value =
                (points[left].value + delta).clamp(config.value_min, config.value_max);
            points[right].value =
                (points[right].value + delta).clamp(config.value_min, config.value_max);
        }
    }
    let Ok(next) = Curve::from_points(points) else {
        return false;
    };
    if next == *curve {
        return false;
    }
    *curve = next;
    drag.changed = true;
    true
}

fn allowed_time_delta(curve: &Curve, selected: &BTreeSet<TimeMus>) -> (f64, f64) {
    let unselected: Vec<f64> = curve
        .points()
        .iter()
        .filter(|point| !selected.contains(&point.time))
        .map(|point| point.time.as_f64())
        .collect();
    let mut min_delta = f64::NEG_INFINITY;
    let mut max_delta = f64::INFINITY;
    for time in selected {
        let beats = time.as_f64();
        if let Some(before) = unselected
            .iter()
            .copied()
            .filter(|candidate| *candidate < beats)
            .reduce(f64::max)
        {
            min_delta = min_delta.max(before - beats + TIME_EPSILON);
        }
        if let Some(after) = unselected
            .iter()
            .copied()
            .filter(|candidate| *candidate > beats)
            .reduce(f64::min)
        {
            max_delta = max_delta.min(after - beats - TIME_EPSILON);
        }
    }
    (min_delta, max_delta)
}

fn add_point(curve: &mut Curve, time: TimeMus, value: f64) -> Option<TimeMus> {
    let mut points = curve.points().to_vec();
    points.push(CurvePoint::new(time, value));
    let next = Curve::from_points(points).ok()?;
    *curve = next;
    Some(time)
}

fn marquee_selection(
    curve: &Curve,
    timeline: TimelineGrid,
    rect: Rect,
    config: &LineEditorConfig,
    marquee: &MarqueeDrag,
) -> BTreeSet<TimeMus> {
    let area = Rect::from_two_pos(marquee.origin, marquee.current);
    curve
        .points()
        .iter()
        .filter(|point| {
            let pos = egui::pos2(
                x_for_time(timeline, rect, point.time),
                y_for_value(config, rect, point.value),
            );
            area.contains(pos)
        })
        .map(|point| point.time)
        .collect()
}

fn line_hit(
    curve: &Curve,
    timeline: TimelineGrid,
    rect: Rect,
    config: &LineEditorConfig,
    pointer: Pos2,
) -> bool {
    let mut line = Vec::new();
    sample_curve(curve, timeline, rect, config, &mut line);
    line.windows(2).any(|pair| {
        let [a, b] = pair else {
            return false;
        };
        distance_to_segment(pointer, *a, *b) <= LINE_HIT_RADIUS
    })
}

fn distance_to_segment(point: Pos2, a: Pos2, b: Pos2) -> f32 {
    let segment = b - a;
    let length_sq = segment.length_sq();
    if length_sq <= f32::EPSILON {
        return (point - a).length();
    }
    let t = ((point - a).dot(segment) / length_sq).clamp(0.0, 1.0);
    (point - (a + segment * t)).length()
}

fn point_at(
    curve: &Curve,
    timeline: TimelineGrid,
    rect: Rect,
    config: &LineEditorConfig,
    pointer: Pos2,
) -> Option<TimeMus> {
    curve.points().iter().find_map(|point| {
        let center = egui::pos2(
            x_for_time(timeline, rect, point.time),
            y_for_value(config, rect, point.value),
        );
        ((center - pointer).length() <= POINT_HIT_RADIUS).then_some(point.time)
    })
}

fn segment_left_at(curve: &Curve, timeline: TimelineGrid, rect: Rect, x: f32) -> Option<TimeMus> {
    let points = curve.points();
    if points.len() < 2 {
        return None;
    }
    for pair in points.windows(2) {
        let x0 = x_for_time(timeline, rect, pair[0].time);
        let x1 = x_for_time(timeline, rect, pair[1].time);
        if x >= x0.min(x1) && x <= x0.max(x1) {
            return Some(pair[0].time);
        }
    }
    let first_x = x_for_time(timeline, rect, points[0].time);
    if x < first_x {
        Some(points[0].time)
    } else {
        Some(points[points.len() - 2].time)
    }
}

fn pointer_x(response: &egui::Response, ui: &egui::Ui, rect: Rect) -> f32 {
    response
        .interact_pointer_pos()
        .or_else(|| ui.input(|input| input.pointer.hover_pos()))
        .unwrap_or(rect.center())
        .x
}

fn pointer_y(response: &egui::Response, ui: &egui::Ui, rect: Rect) -> f32 {
    response
        .interact_pointer_pos()
        .or_else(|| ui.input(|input| input.pointer.hover_pos()))
        .unwrap_or(rect.center())
        .y
}

fn drag_start_pos(ui: &egui::Ui, response: &egui::Response) -> Option<Pos2> {
    ui.input(|input| input.pointer.press_origin())
        .or_else(|| response.interact_pointer_pos())
}

fn x_for_time(timeline: TimelineGrid, rect: Rect, time: TimeMus) -> f32 {
    timeline.x_for_beat(time, rect.left())
}

fn time_for_x(timeline: TimelineGrid, rect: Rect, x: f32) -> TimeMus {
    timeline.beat_for_x(x, rect.left())
}

fn y_for_value(config: &LineEditorConfig, rect: Rect, value: f64) -> f32 {
    let span = value_span(config);
    rect.bottom() - ((value - config.value_min) / span) as f32 * rect.height()
}

fn value_for_y(config: &LineEditorConfig, rect: Rect, y: f32) -> f64 {
    let ratio = f64::from((rect.bottom() - y) / rect.height()).clamp(0.0, 1.0);
    (config.value_min + ratio * value_span(config)).clamp(config.value_min, config.value_max)
}

fn value_span(config: &LineEditorConfig) -> f64 {
    (config.value_max - config.value_min).max(f64::EPSILON)
}

fn clamp_time(time: TimeMus, config: &LineEditorConfig) -> TimeMus {
    let mut beats = time.as_f64();
    if let Some(min) = config.min_time {
        beats = beats.max(min.as_f64());
    }
    if let Some(max) = config.max_time {
        beats = beats.min(max.as_f64());
    }
    beats_to_time(beats)
}
