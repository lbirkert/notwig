use std::f32::consts::PI;

use egui::{Align2, Color32, FontId, Id, Sense, Stroke, Vec2};

use crate::theme;

pub(super) fn knob(
    ui: &mut egui::Ui,
    id: Id,
    norm: &mut f32,
    default_norm: f32,
    value: &str,
    label: &str,
    bipolar: bool,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(78.0, 84.0), Sense::click_and_drag());
    let mut changed = false;
    if let Some(value) = crate::widgets::value_drag(
        ui,
        &response,
        id,
        f64::from(*norm),
        f64::from(default_norm),
        0.0,
        1.0,
        0.005,
        true,
    ) {
        *norm = value as f32;
        changed = true;
    }
    let painter = ui.painter();
    let center = egui::pos2(rect.center().x, rect.top() + 27.0);
    let radius = 20.0;
    painter.circle_filled(center, radius + 4.0, Color32::from_rgb(27, 29, 35));
    painter.circle_stroke(center, radius, Stroke::new(2.0_f32, theme::GRID));
    let rotation = -PI / 2.0;
    let start = -PI * 0.75 + rotation;
    let end = PI * 0.75 + rotation;
    let angle = start + (end - start) * norm.clamp(0.0, 1.0);
    let arc_start = if bipolar { rotation } else { start };
    let steps = 24;
    let mut points = Vec::with_capacity(steps + 1);
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let a = arc_start + (angle - arc_start) * t;
        points.push(egui::pos2(
            center.x + radius * a.cos(),
            center.y + radius * a.sin(),
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
                center.x + radius * angle.cos(),
                center.y + radius * angle.sin(),
            ),
        ],
        Stroke::new(2.0_f32, Color32::WHITE),
    );
    if bipolar {
        painter.line_segment(
            [
                egui::pos2(center.x, center.y - radius - 4.0),
                egui::pos2(center.x, center.y - radius - 1.0),
            ],
            Stroke::new(2.0_f32, theme::TEXT_MUTED),
        );
    }
    painter.text(
        egui::pos2(rect.center().x, rect.top() + 57.0),
        Align2::CENTER_TOP,
        value,
        FontId::monospace(9.5),
        theme::ACCENT,
    );
    painter.text(
        egui::pos2(rect.center().x, rect.bottom() - 1.0),
        Align2::CENTER_BOTTOM,
        label,
        FontId::proportional(11.0),
        theme::TEXT_MUTED,
    );
    changed
}
