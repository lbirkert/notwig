use egui::{Color32, RichText};

use crate::intent::UiIntent;
use crate::runtime::{ProjectPickerAction, RuntimeView};
use crate::theme;

use super::shell::NotwigUi;

impl NotwigUi {
    pub(crate) fn project_picker_window(&mut self, ctx: &egui::Context, runtime: &RuntimeView) {
        let Some(picker) = runtime.project_picker.as_ref() else {
            self.project_picker_confirm_overwrite = false;
            return;
        };
        if picker.epoch != self.project_picker_epoch {
            self.project_picker_epoch = picker.epoch;
            self.project_picker_name.clone_from(&picker.suggested_name);
            self.project_picker_confirm_overwrite = false;
        }

        let title = match picker.action {
            ProjectPickerAction::Open => "Open Project",
            ProjectPickerAction::SaveAs => "Save As",
        };
        let mut open = true;
        egui::Window::new(title)
            .id(egui::Id::new(("project-picker", picker.epoch)))
            .collapsible(false)
            .resizable(false)
            .default_width(360.0)
            .open(&mut open)
            .show(ctx, |ui| {
                if picker.loading {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Loading projects…");
                    });
                }
                if let Some(error) = &picker.error {
                    ui.colored_label(Color32::from_rgb(240, 120, 110), error);
                }

                if picker.projects.is_empty() {
                    if !picker.loading && picker.error.is_none() {
                        ui.label(
                            RichText::new("No saved browser projects yet.")
                                .color(theme::TEXT_MUTED),
                        );
                    }
                } else {
                    ui.label(RichText::new("Projects").color(theme::TEXT_MUTED));
                    egui::ScrollArea::vertical()
                        .max_height(180.0)
                        .show(ui, |ui| {
                            for name in &picker.projects {
                                let selected = picker.action == ProjectPickerAction::SaveAs
                                    && *name == self.project_picker_name;
                                if ui.selectable_label(selected, name).clicked() {
                                    match picker.action {
                                        ProjectPickerAction::Open => {
                                            self.emit(UiIntent::OpenProjectNamed(name.clone()));
                                        }
                                        ProjectPickerAction::SaveAs => {
                                            self.project_picker_name.clone_from(name);
                                            self.project_picker_confirm_overwrite = false;
                                        }
                                    }
                                }
                            }
                        });
                }

                if picker.action == ProjectPickerAction::SaveAs {
                    ui.separator();
                    ui.label("Project name");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.project_picker_name)
                            .hint_text("Project name"),
                    );
                    let name = self.project_picker_name.trim().to_owned();
                    let exists = picker.projects.iter().any(|project| project == &name);
                    if exists {
                        ui.label(
                            RichText::new("A project with this name already exists.")
                                .color(theme::ACCENT),
                        );
                    }
                    ui.horizontal(|ui| {
                        let can_save = !name.is_empty() && !picker.loading;
                        if self.project_picker_confirm_overwrite {
                            if ui.button("Overwrite").clicked() {
                                self.emit(UiIntent::SaveProjectNamed(name.clone()));
                            }
                            if ui.button("Cancel").clicked() {
                                self.project_picker_confirm_overwrite = false;
                            }
                        } else if ui
                            .add_enabled(can_save, egui::Button::new("Save"))
                            .clicked()
                        {
                            if exists {
                                self.project_picker_confirm_overwrite = true;
                            } else {
                                self.emit(UiIntent::SaveProjectNamed(name.clone()));
                            }
                        }
                    });
                }

                if ui.button("Cancel").clicked() {
                    self.emit(UiIntent::CancelProjectPicker);
                }
            });

        if !open {
            self.emit(UiIntent::CancelProjectPicker);
        }
    }
}
