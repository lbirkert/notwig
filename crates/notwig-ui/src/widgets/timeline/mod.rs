mod beat_strip;
mod clip;
mod clip_chrome;
mod drop_preview;
mod grid;
pub(crate) mod interaction;
mod playhead;
mod selection;
mod snap_controls;

use egui::{CornerRadius, Id, Rect, Sense};
use notwig_core::TimeMus;

use crate::theme;

pub(crate) use beat_strip::{BeatStrip, BeatStripAction, draw_playhead};
pub(crate) use clip::*;
pub(crate) use clip_chrome::{ClipChrome, ClipChromeDrag, ClipChromeEdge, draw_clip_chrome};
pub(crate) use drop_preview::draw_drop_preview;
pub(crate) use grid::{TimelineGrid, grid_color};
pub(crate) use playhead::playhead_for_strip;
pub(crate) use selection::{draw_time_selection_fill, draw_time_selection_markers};
pub(crate) use snap_controls::{CONTROLS_WIDTH, show as snap_controls};

pub(crate) fn horizontal_scrollbar(
    ui: &mut egui::Ui,
    bar_rect: Rect,
    track: Rect,
    content_end: f64,
    visible: f64,
    start: TimeMus,
    id: Id,
) -> Option<TimeMus> {
    let painter = ui.painter();
    painter.rect_filled(bar_rect, CornerRadius::ZERO, theme::PANEL_LIGHT);
    let content = content_end.max(visible).max(1.0);
    let start = start.as_f64().clamp(0.0, (content - visible).max(0.0));
    let thumb_x = track.left() + (start / content) as f32 * track.width();
    let thumb_w = ((visible / content) as f32 * track.width())
        .max(24.0)
        .min(track.width());
    let thumb = Rect::from_min_size(
        egui::pos2(thumb_x.min(track.right() - thumb_w), track.top()),
        egui::vec2(thumb_w, track.height()),
    );
    painter.rect_filled(track, CornerRadius::same(3), theme::GRID);
    painter.rect_filled(thumb, CornerRadius::same(3), theme::TEXT_MUTED);

    let response = ui.interact(bar_rect, id, Sense::click_and_drag());
    if (response.clicked() || response.dragged())
        && let Some(pos) = response.interact_pointer_pos()
    {
        let fraction = f64::from(((pos.x - track.left()) / track.width()).clamp(0.0, 1.0));
        let max_start = (content - visible).max(0.0);
        return Some(TimeMus::new(
            ((fraction * max_start) * 1_000_000.0).round() as i64,
            1_000_000,
        ));
    }
    None
}

#[doc(hidden)]
pub mod test_api {
    pub use super::clip::test_api as clip;
    pub use super::grid::TimelineGrid;
    pub use super::snap_controls::test_api as snap_controls;

    pub fn pointer_delta(start: egui::Pos2, current: egui::Pos2, ppb: f32) -> f64 {
        super::interaction::pointer_delta(start, current, ppb)
    }

    pub fn snapped_start(
        absolute_anchor: notwig_core::TimeMus,
        delta: f64,
        grid: notwig_core::TimeMus,
        bypass: bool,
    ) -> notwig_core::TimeMus {
        super::interaction::snapped_start(absolute_anchor, delta, grid, bypass)
    }

    pub fn snapped_shift(
        absolute_anchor: notwig_core::TimeMus,
        delta: f64,
        grid: notwig_core::TimeMus,
        bypass: bool,
    ) -> notwig_core::TimeMus {
        super::interaction::snapped_shift(absolute_anchor, delta, grid, bypass)
    }

    pub fn selection_range(
        anchor: notwig_core::TimeMus,
        current: notwig_core::TimeMus,
    ) -> Option<notwig_core::RangeMus> {
        super::interaction::selection_range(anchor, current)
    }

    pub fn trim_range(
        origin: notwig_core::RangeMus,
        edge: notwig_core::edit::Edge,
        delta: f64,
        grid: notwig_core::TimeMus,
        bypass: bool,
        min_len: notwig_core::TimeMus,
    ) -> notwig_core::RangeMus {
        super::interaction::trim_range(origin, edge, delta, grid, bypass, min_len)
    }

    pub fn navigation_update(
        viewport: &mut crate::Viewport,
        pointer_x: f32,
        content_left: f32,
        zoom_delta: f32,
        middle_down: bool,
        pointer_delta: egui::Vec2,
        scroll_delta: egui::Vec2,
    ) {
        super::interaction::navigation_update(
            viewport,
            pointer_x,
            content_left,
            zoom_delta,
            middle_down,
            pointer_delta,
            scroll_delta,
        );
    }

    pub fn resize_loop_edge(
        start_edge: bool,
        range: notwig_core::RangeMus,
        beat: notwig_core::TimeMus,
    ) -> Option<notwig_core::RangeMus> {
        super::beat_strip::loop_edge_range(
            if start_edge {
                super::beat_strip::LoopEdge::Start
            } else {
                super::beat_strip::LoopEdge::End
            },
            range,
            beat,
        )
    }

    pub fn playhead_for_strip(runtime: &crate::RuntimeView) -> notwig_core::TimeMus {
        super::playhead::playhead_for_strip(runtime)
    }
}
