use egui::RichText;

use crate::theme;

pub(super) fn section_header(ui: &mut egui::Ui, label: &str) {
    ui.label(
        RichText::new(label)
            .small()
            .monospace()
            .strong()
            .color(theme::TEXT_MUTED),
    );
}
