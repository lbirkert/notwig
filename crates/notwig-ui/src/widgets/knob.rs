use egui::{Color32, Rect, Sense, Stroke};

use crate::theme;

/// Draws a rotary knob and handles drag interaction.
///
/// The value is normalized to `0.0..=1.0`. Dragging vertically changes the
/// value; double-click resets to `default_norm`; Cmd/Ctrl+click opens the
/// shared numeric editor. Returns the new normalized value when it changed.
pub(crate) fn knob(
    ui: &mut egui::Ui,
    id: egui::Id,
    rect: Rect,
    value_norm: f32,
    default_norm: f32,
    bipolar: bool,
    tooltip: &str,
) -> Option<f32> {
    let response = ui
        .interact(rect, id, Sense::click_and_drag())
        .on_hover_text(tooltip);
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
    }
    let mut norm = value_norm;
    let mut changed = false;
    if let Some(value) = crate::widgets::value_drag(
        ui,
        &response,
        id.with("knob-drag"),
        f64::from(norm),
        f64::from(default_norm),
        0.0,
        1.0,
        0.005,
        true,
    ) {
        norm = value as f32;
        changed = true;
    }
    let painter = ui.painter().clone();
    draw_knob(&painter, rect, norm, bipolar);
    changed.then_some(norm)
}

fn draw_knob(painter: &egui::Painter, rect: Rect, norm: f32, bipolar: bool) {
    let center = rect.center();
    let radius = 15.0;
    painter.circle_filled(center, radius + 3.0, Color32::from_rgb(29, 31, 37));
    painter.circle_stroke(
        center,
        radius,
        Stroke::new(3.0_f32, Color32::from_rgb(73, 77, 88)),
    );
    let start = std::f32::consts::PI * 0.75;
    let end = std::f32::consts::PI * 2.25;
    let angle = start + (end - start) * norm.clamp(0.0, 1.0);
    let arc_start = if bipolar {
        start + (end - start) * 0.5
    } else {
        start
    };
    let steps = 24;
    let mut points = Vec::with_capacity(steps + 1);
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let angle = arc_start + (angle - arc_start) * t;
        points.push(egui::pos2(
            center.x + radius * angle.cos(),
            center.y + radius * angle.sin(),
        ));
    }
    painter.add(egui::Shape::line(
        points,
        Stroke::new(3.0_f32, theme::ACCENT),
    ));
    painter.line_segment(
        [
            center,
            egui::pos2(
                center.x + (radius - 3.0) * angle.cos(),
                center.y + (radius - 3.0) * angle.sin(),
            ),
        ],
        Stroke::new(2.0_f32, Color32::WHITE),
    );
    if bipolar {
        painter.line_segment(
            [
                egui::pos2(center.x, rect.top() - 1.0),
                egui::pos2(center.x, rect.top() + 1.0),
            ],
            Stroke::new(1.5_f32, theme::TEXT_MUTED),
        );
    }
}
