use egui::{Align2, Color32, CornerRadius, FontId, Sense, Stroke, StrokeKind, Vec2};

use crate::theme;

#[derive(Clone, Copy)]
pub(super) struct Bracket {
    pub(super) left: f32,
    pub(super) right: f32,
    pub(super) y: f32,
    pub(super) depth: usize,
}

pub(super) fn draw_bracket(ui: &egui::Ui, left: f32, right: f32, y: f32, depth: usize) {
    draw_bracket_lines(ui.painter(), left, right, y, nesting_color(depth));
}

fn draw_bracket_lines(painter: &egui::Painter, left: f32, right: f32, y: f32, color: Color32) {
    let stroke = Stroke::new(3.0_f32, color);
    let left = left + 2.0;
    let right = right - 2.0;
    painter.line_segment(
        [egui::pos2(left, y + 7.0), egui::pos2(left, y + 2.0)],
        stroke,
    );
    painter.line_segment(
        [egui::pos2(left, y + 2.0), egui::pos2(right, y + 2.0)],
        stroke,
    );
    painter.line_segment(
        [egui::pos2(right, y + 2.0), egui::pos2(right, y + 7.0)],
        stroke,
    );
}

fn nesting_color(depth: usize) -> Color32 {
    const COLORS: [Color32; 5] = [
        Color32::from_rgb(88, 148, 205),
        Color32::from_rgb(104, 176, 112),
        Color32::from_rgb(204, 156, 72),
        Color32::from_rgb(164, 112, 196),
        Color32::from_rgb(80, 176, 180),
    ];
    COLORS[depth.min(COLORS.len() - 1)]
}

pub(super) fn chain_tab(ui: &mut egui::Ui, expanded: bool, depth: usize) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(84.0, 24.0), Sense::click());
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        CornerRadius {
            nw: 3,
            ne: 3,
            sw: 0,
            se: 0,
        },
        if response.hovered() {
            theme::GRID_STRONG
        } else {
            Color32::from_rgb(20, 21, 25)
        },
    );
    painter.rect_stroke(
        rect,
        CornerRadius {
            nw: 3,
            ne: 3,
            sw: 0,
            se: 0,
        },
        Stroke::new(1.0_f32, theme::GRID),
        StrokeKind::Inside,
    );
    let bracket_left = rect.center().x - 22.0;
    let bracket_right = rect.center().x + 22.0;
    draw_bracket_lines(
        painter,
        bracket_left,
        bracket_right,
        rect.center().y - 7.0,
        nesting_color(depth),
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        "CHAIN",
        FontId::monospace(10.0),
        if expanded {
            Color32::WHITE
        } else {
            theme::TEXT_MUTED
        },
    );
    response.clicked()
}
