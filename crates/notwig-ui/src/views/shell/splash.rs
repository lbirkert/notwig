use egui::Align2;

use crate::theme;

use super::NotwigUi;

impl NotwigUi {
    /// Paints the startup wordmark without taking input or blocking the UI.
    ///
    /// `opacity` is supplied by the app layer so the splash fades in and out
    /// while the workspace and audio engine are already starting underneath.
    pub fn show_startup_splash(&self, ctx: &egui::Context, opacity: f32) {
        let opacity = opacity.clamp(0.0, 1.0);
        if opacity <= 0.0 {
            return;
        }
        let screen = ctx.screen_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("notwig-startup-splash"),
        ));
        painter.rect_filled(
            screen,
            egui::CornerRadius::ZERO,
            with_opacity(theme::APP_BAR, opacity),
        );

        let title_size = (screen.height() * 0.18).clamp(48.0, 132.0);
        let title_pos = screen.center() - egui::vec2(0.0, title_size * 0.08);
        painter.text(
            title_pos,
            Align2::CENTER_CENTER,
            "NOTWIG",
            theme::app_title_font(ctx, title_size),
            with_opacity(theme::TEXT_PRIMARY, opacity),
        );

        let rule_width = (title_size * 3.6).min(screen.width() * 0.72);
        let rule_gap = title_size * 0.58;
        let rule_y = title_pos.y + rule_gap;
        painter.line_segment(
            [
                egui::pos2(title_pos.x - rule_width * 0.5, rule_y),
                egui::pos2(title_pos.x + rule_width * 0.5, rule_y),
            ],
            egui::Stroke::new(1.0_f32, with_opacity(theme::ACCENT, opacity * 0.8)),
        );
        painter.text(
            egui::pos2(title_pos.x, rule_y + title_size * 0.16),
            Align2::CENTER_CENTER,
            "DIGITAL AUDIO WORKSTATION",
            theme::custom_font(ctx, "Geist SemiBold", 11.0),
            with_opacity(theme::TEXT_MUTED, opacity),
        );
    }
}

fn with_opacity(color: egui::Color32, opacity: f32) -> egui::Color32 {
    let alpha = (f32::from(color.a()) * opacity).round().clamp(0.0, 255.0) as u8;
    egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}
