use egui::Pos2;
use notwig_core::edit::Edge;
use notwig_core::{RangeMus, TimeMus};

use crate::state::Viewport;
use crate::widgets::beats_to_time;

pub(crate) fn pointer_delta(start: Pos2, current: Pos2, ppb: f32) -> f64 {
    if ppb <= 0.0 {
        return 0.0;
    }
    f64::from(current.x - start.x) / f64::from(ppb)
}

pub(crate) fn snapped_start(
    absolute_anchor: TimeMus,
    delta: f64,
    grid: TimeMus,
    bypass: bool,
) -> TimeMus {
    let beat = beats_to_time(absolute_anchor.as_f64() + delta);
    crate::widgets::snap_time(beat, grid, bypass)
}

pub(crate) fn snapped_shift(
    absolute_anchor: TimeMus,
    delta: f64,
    grid: TimeMus,
    bypass: bool,
) -> TimeMus {
    snapped_start(absolute_anchor, delta, grid, bypass) - absolute_anchor
}

pub(crate) fn selection_range(anchor: TimeMus, current: TimeMus) -> Option<RangeMus> {
    let (start, end) = if anchor <= current {
        (anchor, current)
    } else {
        (current, anchor)
    };
    RangeMus::new(start, end)
        .ok()
        .filter(|range| !range.is_empty())
}

pub(crate) fn drag_start_pos(ui: &egui::Ui, response: &egui::Response) -> Option<Pos2> {
    ui.input(|i| i.pointer.press_origin())
        .or_else(|| response.interact_pointer_pos())
}

pub(crate) fn drag_current_pos(ui: &egui::Ui, response: &egui::Response, fallback: Pos2) -> Pos2 {
    response
        .interact_pointer_pos()
        .or_else(|| ui.input(|i| i.pointer.hover_pos()))
        .unwrap_or(fallback)
}

pub(crate) fn trim_range(
    origin: RangeMus,
    edge: Edge,
    delta: f64,
    grid: TimeMus,
    bypass: bool,
    min_len: TimeMus,
) -> RangeMus {
    let min_len = min_len.max(TimeMus::zero());
    match edge {
        Edge::Start => {
            let latest = (origin.end - min_len).max(TimeMus::zero());
            let start = snap_beats(origin.start.as_f64() + delta, grid, bypass)
                .max(TimeMus::zero())
                .min(latest);
            RangeMus::new(start, origin.end).unwrap_or(origin)
        }
        Edge::End => {
            let end =
                snap_beats(origin.end.as_f64() + delta, grid, bypass).max(origin.start + min_len);
            RangeMus::new(origin.start, end).unwrap_or(origin)
        }
    }
}

fn snap_beats(beats: f64, grid: TimeMus, bypass: bool) -> TimeMus {
    crate::widgets::snap_time(beats_to_time(beats), grid, bypass)
}

pub(crate) fn navigation_update(
    viewport: &mut Viewport,
    pointer_x: f32,
    content_left: f32,
    zoom_delta: f32,
    middle_down: bool,
    pointer_delta: egui::Vec2,
    scroll_delta: egui::Vec2,
) {
    let ppb = viewport
        .pixels_per_beat
        .clamp(Viewport::MIN_PPB, Viewport::MAX_PPB);
    if (zoom_delta - 1.0).abs() > 1e-4 {
        viewport.zoom_at(pointer_x, content_left, zoom_delta);
    }
    if middle_down {
        if pointer_delta.x.abs() > 0.0 {
            viewport.pan_beats(-f64::from(pointer_delta.x) / f64::from(ppb));
        }
        if pointer_delta.y.abs() > 0.0 {
            viewport.zoom_at(
                pointer_x,
                content_left,
                (-f64::from(pointer_delta.y) * 0.012).exp() as f32,
            );
        }
    }
    if !middle_down && scroll_delta.x.abs() > scroll_delta.y.abs() && scroll_delta.x.abs() > 0.01 {
        viewport.pan_beats(-f64::from(scroll_delta.x) / f64::from(ppb));
    }
}

pub(crate) fn timeline_navigation(
    ui: &egui::Ui,
    content: egui::Rect,
    content_left: f32,
    viewport: &mut Viewport,
) {
    timeline_navigation_impl(ui, content, content_left, viewport, false);
}

pub(crate) fn timeline_navigation_unbounded(
    ui: &egui::Ui,
    content: egui::Rect,
    content_left: f32,
    viewport: &mut Viewport,
) {
    timeline_navigation_impl(ui, content, content_left, viewport, true);
}

fn timeline_navigation_impl(
    ui: &egui::Ui,
    content: egui::Rect,
    content_left: f32,
    viewport: &mut Viewport,
    allow_negative: bool,
) {
    if !ui.rect_contains_pointer(content) {
        return;
    }
    let Some(pointer) = ui.input(|i| i.pointer.hover_pos()) else {
        return;
    };
    let zoom_delta = ui.input(egui::InputState::zoom_delta);
    let middle_down = ui.input(|i| i.pointer.middle_down());
    let pointer_delta = ui.input(|i| i.pointer.delta());
    let scroll_delta = ui.input(|i| i.smooth_scroll_delta);
    let ppb = viewport
        .pixels_per_beat
        .clamp(Viewport::MIN_PPB, Viewport::MAX_PPB);
    if (zoom_delta - 1.0).abs() > 1e-4 {
        if allow_negative {
            viewport.zoom_at_unbounded(pointer.x, content_left, zoom_delta);
        } else {
            viewport.zoom_at(pointer.x, content_left, zoom_delta);
        }
    }
    if middle_down {
        if pointer_delta.x.abs() > 0.0 {
            let delta = -f64::from(pointer_delta.x) / f64::from(ppb);
            if allow_negative {
                viewport.pan_beats_unbounded(delta);
            } else {
                viewport.pan_beats(delta);
            }
        }
        if pointer_delta.y.abs() > 0.0 {
            let factor = (-f64::from(pointer_delta.y) * 0.012).exp() as f32;
            if allow_negative {
                viewport.zoom_at_unbounded(pointer.x, content_left, factor);
            } else {
                viewport.zoom_at(pointer.x, content_left, factor);
            }
        }
    }
    if !middle_down && scroll_delta.x.abs() > scroll_delta.y.abs() && scroll_delta.x.abs() > 0.01 {
        let delta = -f64::from(scroll_delta.x) / f64::from(ppb);
        if allow_negative {
            viewport.pan_beats_unbounded(delta);
        } else {
            viewport.pan_beats(delta);
        }
    }
}
