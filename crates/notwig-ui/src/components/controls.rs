use egui::RichText;

use crate::palette;

/// Draws a compact uppercase section heading.
pub fn section(ui: &mut egui::Ui, title: &str) {
    ui.label(
        RichText::new(title.to_uppercase())
            .font(egui::FontId::monospace(9.0))
            .color(palette::TEXT_MUTED),
    );
    ui.add_space(3.0);
}

/// Draws a muted label.
pub fn label(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(11.0).color(palette::TEXT_MUTED));
}

/// Draws a horizontal divider.
pub fn divider(ui: &mut egui::Ui) {
    ui.separator();
}

/// Draws a standard button.
pub fn button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(text).size(11.0)))
}

/// Draws a selectable toggle and reports whether it was clicked.
pub fn toggle(ui: &mut egui::Ui, active: bool, text: &str) -> bool {
    ui.selectable_label(active, RichText::new(text).size(11.0))
        .clicked()
}

/// Draws a dropdown and returns the newly selected index.
pub fn dropdown(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    selected: usize,
    options: &[&str],
) -> Option<usize> {
    let mut current = selected;
    let mut chosen = None;
    egui::ComboBox::from_id_salt(id)
        .selected_text(options.get(selected).copied().unwrap_or(""))
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for (index, option) in options.iter().enumerate() {
                if ui.selectable_value(&mut current, index, *option).clicked() {
                    chosen = Some(index);
                }
            }
        });
    chosen
}
