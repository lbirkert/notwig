use egui::{Color32, CornerRadius, Rect};

/// Draws a stereo peak meter with a -60 dB floor.
pub fn draw_stereo_meter(painter: &egui::Painter, rect: Rect, peak_l: f32, peak_r: f32) {
    draw_stereo_meter_with_peak(painter, rect, peak_l, peak_r, None);
}

/// Draws a stereo peak meter and an optional peak-hold marker.
pub fn draw_stereo_meter_with_peak(
    painter: &egui::Painter,
    rect: Rect,
    peak_l: f32,
    peak_r: f32,
    peak_hold: Option<f32>,
) {
    let gap = 2.0;
    let bar_w = ((rect.width() - gap) * 0.5).max(1.0);
    for (index, peak) in [peak_l, peak_r].into_iter().enumerate() {
        let x = rect.left() + index as f32 * (bar_w + gap);
        let bar = Rect::from_min_max(
            egui::pos2(x, rect.top()),
            egui::pos2(x + bar_w, rect.bottom()),
        );
        painter.rect_filled(bar, CornerRadius::same(1), Color32::from_rgb(13, 14, 17));
        let db = 20.0 * peak.max(1e-6).log10();
        let level = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
        let top = bar.bottom() - bar.height() * level;
        let visible = Rect::from_min_max(egui::pos2(x, top), egui::pos2(x + bar_w, bar.bottom()));
        let green = Rect::from_min_max(
            egui::pos2(x, bar.top() + bar.height() * 0.25),
            egui::pos2(x + bar_w, bar.bottom()),
        );
        let yellow = Rect::from_min_max(
            egui::pos2(x, bar.top() + bar.height() * 0.08),
            egui::pos2(x + bar_w, bar.top() + bar.height() * 0.25),
        );
        let red = Rect::from_min_max(
            egui::pos2(x, bar.top()),
            egui::pos2(x + bar_w, bar.top() + bar.height() * 0.08),
        );
        for (segment, color) in [
            (green, Color32::from_rgb(91, 196, 88)),
            (yellow, Color32::from_rgb(218, 165, 61)),
            (red, Color32::from_rgb(224, 80, 66)),
        ] {
            let clipped = segment.intersect(visible);
            if clipped.height() > 0.0 {
                painter.rect_filled(clipped, CornerRadius::ZERO, color);
            }
        }
        if let Some(hold) = peak_hold {
            let hold_db = 20.0 * hold.max(1e-6).log10();
            let hold_level = ((hold_db + 60.0) / 60.0).clamp(0.0, 1.0);
            let y = bar.bottom() - bar.height() * hold_level;
            painter.line_segment(
                [egui::pos2(x, y), egui::pos2(x + bar_w, y)],
                egui::Stroke::new(1.0_f32, Color32::WHITE),
            );
        }
    }
}
