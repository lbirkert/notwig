use egui::RichText;
use notwig_core::ProjectDoc;

use crate::icons::Icon;
use crate::runtime::RuntimeView;
use crate::state::OpenedEditor;
use crate::theme;

use super::NotwigUi;

impl NotwigUi {
    pub(super) fn status_bar(
        &mut self,
        ctx: &egui::Context,
        doc: &ProjectDoc,
        _runtime: &RuntimeView,
    ) {
        egui::TopBottomPanel::bottom("notwig_status")
            .exact_height(40.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL_LIGHT)
                    .inner_margin(egui::Margin::symmetric(8, 4)),
            )
            .show(ctx, |ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), 32.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        if footer_icon(ui, Icon::Info, self.show_info, "Selection info").clicked() {
                            self.show_info = !self.show_info;
                        }
                        ui.separator();
                        if footer_icon(
                            ui,
                            Icon::Sliders,
                            self.currently_opened == Some(OpenedEditor::DeviceEditor),
                            "Toggle device editor",
                        )
                        .clicked()
                        {
                            self.toggle_device_editor();
                        }
                        if footer_icon(
                            ui,
                            Icon::Clip,
                            matches!(
                                self.currently_opened,
                                Some(OpenedEditor::ClipEditor | OpenedEditor::MidiEditor)
                            ),
                            "Toggle clip / MIDI editor",
                        )
                        .clicked()
                        {
                            self.toggle_clip_editor(doc);
                        }
                        ui.add_space(12.0);
                        let (action, tooltip) =
                            if self.currently_opened == Some(OpenedEditor::DeviceEditor) {
                                ("Hide Device Editor", "Close the device editor")
                            } else {
                                ("Show Device Editor", "Open the device editor")
                            };
                        if footer_action(ui, action, tooltip) {
                            self.toggle_device_editor();
                        }
                    },
                );
            });
    }
}

fn footer_icon(ui: &mut egui::Ui, icon: Icon, selected: bool, tooltip: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::click());
    let painter = ui.painter();
    if selected || response.hovered() {
        painter.rect_filled(
            rect,
            egui::CornerRadius::same(3),
            if selected {
                theme::ACCENT_DIM
            } else {
                theme::GRID_STRONG
            },
        );
    }
    let color = if selected {
        theme::ACCENT
    } else {
        theme::TEXT_MUTED
    };
    crate::icons::paint(painter, rect, icon, color);
    response.on_hover_text(tooltip)
}

fn footer_action(ui: &mut egui::Ui, action: &str, tooltip: &str) -> bool {
    ui.add(egui::Button::new(RichText::new(action).color(theme::TEXT_MUTED)).frame(false))
        .on_hover_text(tooltip)
        .clicked()
}
