use egui::{Color32, Rect};
use notwig_core::ProjectDoc;

use crate::icons::Icon;
use crate::runtime::RuntimeView;
use crate::theme;

use super::NotwigUi;
use super::top_bar::request_close_project;

const APP_BAR_HEIGHT: f32 = 28.0;

impl NotwigUi {
    pub(super) fn app_bar(ctx: &egui::Context, doc: &ProjectDoc, runtime: &RuntimeView) {
        egui::TopBottomPanel::top("notwig_app_bar")
            .exact_height(APP_BAR_HEIGHT)
            .frame(
                egui::Frame::new()
                    .fill(theme::APP_BAR)
                    .inner_margin(egui::Margin::ZERO),
            )
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                let painter = ui.painter_at(rect);
                painter.line_segment(
                    [
                        egui::pos2(rect.left(), rect.bottom() - 0.5),
                        egui::pos2(rect.right(), rect.bottom() - 0.5),
                    ],
                    egui::Stroke::new(1.0_f32, theme::GRID_STRONG),
                );
                let drag = ui.interact(
                    rect,
                    egui::Id::new("notwig-app-bar-drag"),
                    egui::Sense::click_and_drag(),
                );
                #[cfg(not(target_arch = "wasm32"))]
                {
                    if drag.drag_started() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                    if drag.double_clicked() {
                        let maximized =
                            ctx.input(|input| input.viewport().maximized.unwrap_or(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                    }
                }
                #[cfg(target_arch = "wasm32")]
                let _ = &drag;

                let dirty = if runtime.dirty { "*" } else { "" };
                let project_name = project_display_name(doc, runtime);
                let project_label = format!("{project_name}{dirty}");
                let card_font = theme::custom_font(ctx, "Geist Medium", 10.5);
                let card_galley =
                    painter.layout_no_wrap(project_label, card_font, theme::TEXT_PRIMARY);
                let card_height = 18.0;
                let close_size = 16.0;
                let card_width =
                    (card_galley.size().x + 12.0 + close_size + 4.0).clamp(80.0, 240.0);
                let card_left = if cfg!(all(target_os = "macos", not(target_arch = "wasm32"))) {
                    78.0
                } else {
                    10.0
                };
                let card_rect = Rect::from_min_size(
                    egui::pos2(rect.left() + card_left, rect.center().y - card_height * 0.5),
                    egui::vec2(card_width, card_height),
                );
                let card = ui.interact(
                    card_rect,
                    egui::Id::new("notwig-project-card"),
                    egui::Sense::hover(),
                );
                let close_rect = Rect::from_center_size(
                    egui::pos2(
                        card_rect.right() - close_size * 0.5 - 2.0,
                        card_rect.center().y,
                    ),
                    egui::Vec2::splat(close_size),
                );
                let close = ui.interact(
                    close_rect,
                    egui::Id::new("notwig-project-card-close"),
                    egui::Sense::click(),
                );
                let close = close.on_hover_cursor(egui::CursorIcon::PointingHand);
                painter.rect(
                    card_rect,
                    egui::CornerRadius::same(5),
                    if card.hovered() {
                        theme::GRID
                    } else {
                        theme::APP_BAR_CARD
                    },
                    egui::Stroke::new(1.0_f32, theme::GRID_STRONG),
                    egui::StrokeKind::Inside,
                );
                let name_clip = Rect::from_min_max(
                    card_rect.min,
                    egui::pos2(close_rect.left() - 4.0, card_rect.max.y),
                );
                painter.with_clip_rect(name_clip).galley(
                    egui::pos2(
                        card_rect.left() + 6.0,
                        card_rect.center().y - card_galley.size().y * 0.5,
                    ),
                    card_galley,
                    theme::TEXT_PRIMARY,
                );
                if close.hovered() {
                    painter.rect_filled(
                        close_rect,
                        egui::CornerRadius::same(3),
                        theme::GRID_STRONG,
                    );
                }
                let close_color = if close.hovered() {
                    theme::TEXT_PRIMARY
                } else {
                    theme::TEXT_MUTED
                };
                painter.text(
                    close_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    Icon::Close.glyph(),
                    theme::icon_font(ctx, 10.0),
                    close_color,
                );
                if close.clicked() {
                    request_close_project(ctx);
                }
                if !close.hovered()
                    && let Some(path) = &runtime.bundle_path
                {
                    card.on_hover_text(path.display().to_string());
                }
                close.on_hover_text("Close project");

                let brand = crate::widgets::brand::icon(ctx, true);
                let icon_height = 14.0;
                let brand_size = brand.size_vec2();
                let icon_width = icon_height * (brand_size.x / brand_size.y.max(1.0));
                let title = painter.layout_no_wrap(
                    "NOTWIG".to_owned(),
                    theme::app_title_font(ctx, theme::APP_TITLE_SIZE),
                    theme::TEXT_PRIMARY,
                );
                let gap = 6.0;
                let content_width = icon_width + gap + title.size().x;
                let start_x = rect.center().x - content_width * 0.5;
                let icon_rect = Rect::from_center_size(
                    egui::pos2(start_x + icon_width * 0.5, rect.center().y),
                    egui::vec2(icon_width, icon_height),
                );
                painter.image(
                    brand.id(),
                    icon_rect,
                    Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
                painter.galley(
                    egui::pos2(
                        start_x + icon_width + gap,
                        rect.center().y - title.size().y * 0.5,
                    ),
                    title,
                    theme::TEXT_PRIMARY,
                );
            });
    }
}

fn project_display_name(doc: &ProjectDoc, runtime: &RuntimeView) -> String {
    runtime
        .bundle_path
        .as_ref()
        .and_then(|path| path.file_stem())
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .map_or_else(|| doc.meta.name.clone(), str::to_owned)
}
