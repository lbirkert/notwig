use egui::Align2;

use crate::theme;

use super::NotwigUi;

impl NotwigUi {
    /// Paints the startup wordmark without taking input or blocking the UI.
    ///
    /// `content_opacity` fades the logo/wordmark and `background_opacity`
    /// fades the backdrop. The app keeps the backdrop opaque on entry and only
    /// applies the exit fade, so the workspace never shows through while the
    /// splash is appearing.
    pub fn show_startup_splash(
        &self,
        ctx: &egui::Context,
        content_opacity: f32,
        background_opacity: f32,
    ) {
        let content_opacity = content_opacity.clamp(0.0, 1.0);
        let background_opacity = background_opacity.clamp(0.0, 1.0);
        let screen = ctx.screen_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("notwig-startup-splash"),
        ));
        if background_opacity > 0.0 {
            painter.rect_filled(
                screen,
                egui::CornerRadius::ZERO,
                with_opacity(theme::APP_BAR, background_opacity),
            );
        }
        if content_opacity <= 0.0 {
            return;
        }

        let title_size = (screen.height() * 0.18).clamp(48.0, 132.0);
        let center = screen.center() - egui::vec2(0.0, title_size * 0.08);
        let title = painter.layout_no_wrap(
            "NOTWIG".to_owned(),
            theme::app_title_font(ctx, title_size),
            with_opacity(theme::TEXT_PRIMARY, content_opacity),
        );

        let brand = crate::widgets::brand::icon(ctx, true);
        let brand_size = brand.size_vec2();
        let logo_height = title.size().y * 0.72;
        let logo_width = logo_height * (brand_size.x / brand_size.y.max(1.0));
        let gap = title_size * 0.28;
        let group_width = logo_width + gap + title.size().x;
        let start_x = center.x - group_width * 0.5;
        let logo_rect = egui::Rect::from_center_size(
            egui::pos2(start_x + logo_width * 0.5, center.y),
            egui::vec2(logo_width, logo_height),
        );
        painter.image(
            brand.id(),
            logo_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            with_opacity(egui::Color32::WHITE, content_opacity),
        );
        painter.galley(
            egui::pos2(start_x + logo_width + gap, center.y - title.size().y * 0.5),
            title,
            with_opacity(theme::TEXT_PRIMARY, content_opacity),
        );

        let rule_width = (title_size * 3.6).min(screen.width() * 0.72);
        let rule_y = center.y + title_size * 0.58;
        painter.line_segment(
            [
                egui::pos2(center.x - rule_width * 0.5, rule_y),
                egui::pos2(center.x + rule_width * 0.5, rule_y),
            ],
            egui::Stroke::new(1.0_f32, with_opacity(theme::ACCENT, content_opacity * 0.8)),
        );
        painter.text(
            egui::pos2(center.x, rule_y + title_size * 0.16),
            Align2::CENTER_CENTER,
            "DIGITAL AUDIO WORKSTATION",
            theme::custom_font(ctx, "Geist SemiBold", 11.0),
            with_opacity(theme::TEXT_MUTED, content_opacity),
        );
    }
}

fn with_opacity(color: egui::Color32, opacity: f32) -> egui::Color32 {
    let alpha = (f32::from(color.a()) * opacity).round().clamp(0.0, 255.0) as u8;
    egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}
