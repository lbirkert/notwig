use egui::{Color32, Vec2};

pub(crate) struct PanelConfig {
    pub id: &'static str,
    pub resizable: bool,
    pub default_height: f32,
    pub min_height: f32,
    pub max_height: f32,
    pub collapse_height: f32,
    pub handle_id: &'static str,
    pub handle_tooltip: &'static str,
}

pub(crate) fn bottom_panel(
    ctx: &egui::Context,
    config: PanelConfig,
    content: impl FnOnce(&mut egui::Ui) -> bool,
) -> bool {
    let mut open = true;
    let panel_id = egui::Id::new(config.id);
    let panel = egui::TopBottomPanel::bottom(panel_id).frame(
        egui::Frame::new()
            .fill(Color32::BLACK)
            .inner_margin(egui::Margin::symmetric(6, 4)),
    );
    let panel = if config.resizable {
        // The panel's own resize handle is the only affordance. Let it shrink
        // past the collapse threshold so a downward drag can close the panel.
        let drag_min = config.min_height.min(config.collapse_height - 1.0).max(1.0);
        panel
            .resizable(true)
            .default_height(config.default_height)
            .min_height(drag_min)
            .max_height(config.max_height)
    } else {
        panel.resizable(false).exact_height(config.default_height)
    };
    let response = panel.show(ctx, |ui| {
        // Keep the panel's stored height stable: egui records the content rect,
        // so content that allocates less than the panel would shrink it every frame.
        let available = ui.available_size();
        ui.set_min_size(available);
        if !config.resizable
            && crate::widgets::collapse_handle(
                ui,
                Vec2::new(ui.available_width(), 6.0),
                false,
                config.handle_id,
                config.handle_tooltip,
            )
        {
            open = false;
        }
        if !content(ui) {
            open = false;
        }
    });
    if config.resizable && open && response.response.rect.height() < config.collapse_height {
        open = false;
    }
    if !open && config.resizable {
        // egui persists the last panel height. Restore the default so reopening
        // after a collapse does not immediately collapse again.
        reset_panel_height(ctx, panel_id, config.default_height);
    }
    open
}

fn reset_panel_height(ctx: &egui::Context, panel_id: egui::Id, height: f32) {
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(0.0, height));
    ctx.data_mut(|data| {
        data.insert_persisted(panel_id, egui::containers::panel::PanelState { rect });
    });
}
