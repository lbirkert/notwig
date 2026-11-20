use egui::{Align2, Color32, CornerRadius, FontId, Id, Painter, Pos2, Rect, Sense, Stroke, Ui};
use notwig_core::edit::Edge;
use notwig_core::{RangeMus, TimeMus};

use crate::timeline::TimelineGrid;

const EDGE_HANDLE_WIDTH: f32 = 14.0;
const EDGE_HANDLE_TOP_INSET: f32 = 14.0;

fn min_edge_length() -> TimeMus {
    TimeMus::new(1, 16)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClipChromeEdge {
    Start,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ClipChromeDrag {
    pub edge: ClipChromeEdge,
    pub start_pointer: Pos2,
    pub origin: RangeMus,
}

pub(crate) struct ClipChrome<'a> {
    pub grid: TimelineGrid,
    pub content: Rect,
    pub ruler: Rect,
    pub lanes: Rect,
    pub range: RangeMus,
    pub origin: RangeMus,
    pub id_salt: Id,
    pub ppb: f32,
    pub snap_step: TimeMus,
    pub bypass: bool,
    pub start_label: &'a str,
    pub end_label: &'a str,
    pub start_color: Color32,
    pub end_color: Color32,
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub(crate) struct ClipChromeResponse {
    pub preview: Option<RangeMus>,
    pub committed: Option<RangeMus>,
}

pub(crate) fn draw_clip_chrome(
    ui: &mut Ui,
    painter: &Painter,
    chrome: ClipChrome<'_>,
    drag: &mut Option<ClipChromeDrag>,
) -> ClipChromeResponse {
    draw_clip_marker(
        painter,
        chrome.grid,
        chrome.content,
        chrome.ruler,
        chrome.lanes,
        chrome.range.start,
        chrome.start_label,
        chrome.start_color,
        Align2::LEFT_TOP,
    );
    draw_clip_marker(
        painter,
        chrome.grid,
        chrome.content,
        chrome.ruler,
        chrome.lanes,
        chrome.range.end,
        chrome.end_label,
        chrome.end_color,
        Align2::RIGHT_TOP,
    );

    let mut response = ClipChromeResponse::default();
    for (edge, beat, label) in [
        (ClipChromeEdge::Start, chrome.range.start, "start"),
        (ClipChromeEdge::End, chrome.range.end, "end"),
    ] {
        let x = chrome.grid.x_for_beat(beat, chrome.content.left());
        let handle_top = (chrome.ruler.top() + EDGE_HANDLE_TOP_INSET).min(chrome.ruler.bottom());
        let handle = Rect::from_min_max(
            egui::pos2(x - EDGE_HANDLE_WIDTH * 0.5, handle_top),
            egui::pos2(x + EDGE_HANDLE_WIDTH * 0.5, chrome.ruler.bottom()),
        );
        let edge_response = ui.interact(handle, chrome.id_salt.with(label), Sense::drag());
        if edge_response.hovered() || edge_response.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            painter.rect_filled(
                Rect::from_min_max(
                    egui::pos2(x - 2.0, handle_top),
                    egui::pos2(x + 2.0, chrome.lanes.bottom()),
                ),
                CornerRadius::ZERO,
                Color32::WHITE,
            );
        }
        if edge_response.drag_started()
            && let Some(pointer) = super::interaction::drag_start_pos(ui, &edge_response)
        {
            *drag = Some(ClipChromeDrag {
                edge,
                start_pointer: pointer,
                origin: chrome.origin,
            });
        }
        let mut preview = None;
        if edge_response.dragged()
            && let Some(active) = *drag
        {
            let release =
                super::interaction::drag_current_pos(ui, &edge_response, active.start_pointer);
            preview = Some(edge_preview_range(
                active,
                release,
                chrome.ppb,
                chrome.snap_step,
                chrome.bypass,
            ));
            response.preview = preview;
        }
        if edge_response.drag_stopped()
            && let Some(active) = drag.take()
        {
            let range = preview.unwrap_or_else(|| {
                let release =
                    super::interaction::drag_current_pos(ui, &edge_response, active.start_pointer);
                edge_preview_range(active, release, chrome.ppb, chrome.snap_step, chrome.bypass)
            });
            if range != active.origin {
                response.committed = Some(range);
            }
        }
    }
    response
}

fn edge_preview_range(
    drag: ClipChromeDrag,
    pointer: Pos2,
    ppb: f32,
    grid: TimeMus,
    bypass: bool,
) -> RangeMus {
    let delta = f64::from(pointer.x - drag.start_pointer.x) / f64::from(ppb);
    let edge = match drag.edge {
        ClipChromeEdge::Start => Edge::Start,
        ClipChromeEdge::End => Edge::End,
    };
    super::interaction::trim_range(drag.origin, edge, delta, grid, bypass, min_edge_length())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_clip_marker(
    painter: &Painter,
    grid: TimelineGrid,
    content: Rect,
    ruler: Rect,
    _lanes: Rect,
    beat: TimeMus,
    label: &str,
    color: Color32,
    align: Align2,
) {
    let x = grid.x_for_beat(beat, content.left());
    if x < content.left() - 1.0 || x > content.right() + 1.0 {
        return;
    }
    let center_y = ruler.bottom() - 7.0;
    let half_height = 7.0;
    let width = 11.0;
    let points = if align == Align2::RIGHT_TOP {
        vec![
            egui::pos2(x, center_y - half_height),
            egui::pos2(x, center_y + half_height),
            egui::pos2(x - width, center_y),
        ]
    } else {
        vec![
            egui::pos2(x, center_y - half_height),
            egui::pos2(x, center_y + half_height),
            egui::pos2(x + width, center_y),
        ]
    };
    painter.add(egui::Shape::convex_polygon(
        points,
        color,
        Stroke::new(1.0_f32, color.gamma_multiply(0.62)),
    ));
    if !label.is_empty() {
        painter.text(
            egui::pos2(
                if align == Align2::RIGHT_TOP {
                    x - 3.0
                } else {
                    x + 3.0
                },
                ruler.top(),
            ),
            align,
            label,
            FontId::monospace(8.0),
            color,
        );
    }
}
