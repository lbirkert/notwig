use egui::RichText;

use crate::theme;

pub(crate) fn section_heading(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .strong()
            .size(13.0)
            .color(theme::TEXT_MUTED),
    );
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
        ui.label(RichText::new(label).size(13.0));
        egui::ComboBox::from_id_salt(id_salt)
            .selected_text(selected_text)
            .width(360.0)
            .show_ui(ui, |ui| {
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
