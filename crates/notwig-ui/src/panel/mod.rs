use egui::{Color32, Vec2};

pub(crate) mod clip_editor;
pub(crate) mod device;

pub(crate) struct PanelConfig {
    pub id: &'static str,
    pub resizable: bool,
    pub default_height: f32,
    pub min_height: f32,
    pub max_height: f32,
    pub handle_id: &'static str,
    pub handle_tooltip: &'static str,
}

pub(crate) fn bottom_panel(
    ctx: &egui::Context,
    config: PanelConfig,
    content: impl FnOnce(&mut egui::Ui) -> bool,
) -> bool {
    let mut open = true;
    let panel = egui::TopBottomPanel::bottom(config.id).frame(
        egui::Frame::new()
            .fill(Color32::BLACK)
            .inner_margin(egui::Margin::symmetric(6, 4)),
    );
    let panel = if config.resizable {
        panel
            .resizable(true)
            .default_height(config.default_height)
            .min_height(config.min_height)
            .max_height(config.max_height)
    } else {
        panel.resizable(false).exact_height(config.default_height)
    };
    panel.show(ctx, |ui| {
        if crate::widgets::collapse_handle(
            ui,
            Vec2::new(ui.available_width(), 6.0),
            false,
            config.handle_id,
            config.handle_tooltip,
        ) {
            open = false;
        }
        if !content(ui) {
            open = false;
        }
    });
    open
}
