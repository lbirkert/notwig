use egui::RichText;

use crate::theme;

pub(crate) fn section_heading(ui: &mut egui::Ui, text: &str) {
    ui.label(theme::heading(ui.ctx(), text));
    ui.add_space(6.0);
}

pub(crate) fn select_row(
    ui: &mut egui::Ui,
    label: &str,
    id_salt: &str,
    selected: &mut String,
    selected_text: impl Into<egui::WidgetText>,
    options: Vec<(String, String)>,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(96.0, ui.spacing().interact_size.y),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(RichText::new(label).size(13.0));
            },
        );
        let width = ui.available_width().clamp(160.0, 460.0);
        combo(ui, id_salt, selected_text, width, |ui| {
            for (value, text) in options {
                if ui.selectable_label(*selected == value, text).clicked() {
                    *selected = value;
                    changed = true;
                }
            }
        });
    });
    ui.add_space(8.0);
    changed
}

pub(crate) fn combo(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash,
    selected_text: impl Into<egui::WidgetText>,
    width: f32,
    content: impl FnOnce(&mut egui::Ui),
) {
    egui::ComboBox::from_id_salt(id_salt)
        .selected_text(selected_text)
        .width(width)
        .truncate()
        .show_ui(ui, content);
}
