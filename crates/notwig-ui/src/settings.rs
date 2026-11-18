use egui::{Color32, RichText};

use crate::form;
use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::shell::NotwigUi;
use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsCategory {
    #[default]
    Audio,
    General,
    Appearance,
}

impl SettingsCategory {
    fn label(self) -> &'static str {
        match self {
            Self::Audio => "Audio",
            Self::General => "General",
            Self::Appearance => "Appearance",
        }
    }
}

impl NotwigUi {
    pub(crate) fn open_settings(&mut self, runtime: &RuntimeView) {
        self.settings_open = true;
        self.settings_category = SettingsCategory::Audio;
        self.audio_draft = runtime.audio_settings.current.clone();
    }

    pub(crate) fn settings_window(&mut self, ctx: &egui::Context, runtime: &RuntimeView) {
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.settings_open = false;
            return;
        }
        let mut open = self.settings_open;
        egui::Window::new("Settings")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([780.0, 560.0])
            .min_size([560.0, 360.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(190.0, ui.available_height()),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.add_space(8.0);
                            for category in [
                                SettingsCategory::General,
                                SettingsCategory::Audio,
                                SettingsCategory::Appearance,
                            ] {
                                let selected = self.settings_category == category;
                                if ui
                                    .selectable_label(
                                        selected,
                                        RichText::new(category.label()).size(14.0),
                                    )
                                    .clicked()
                                {
                                    self.settings_category = category;
                                }
                            }
                            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                                ui.add_space(14.0);
                                if ui.button("Close").clicked() {
                                    self.settings_open = false;
                                }
                            });
                        },
                    );
                    ui.separator();
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), ui.available_height()),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            egui::ScrollArea::vertical()
                                .id_salt("settings-content")
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.vertical(|ui| {
                                        ui.add_space(10.0);
                                        ui.set_max_width(560.0);
                                        match self.settings_category {
                                            SettingsCategory::Audio => {
                                                self.audio_settings_page(ui, runtime);
                                            }
                                            other => {
                                                ui.heading(other.label());
                                                ui.add_space(8.0);
                                                ui.label(
                                                    RichText::new(
                                                        "This category is not available yet.",
                                                    )
                                                    .color(theme::TEXT_MUTED),
                                                );
                                            }
                                        }
                                        ui.add_space(20.0);
                                    });
                                });
                        },
                    );
                });
            });
        self.settings_open = open;
    }

    fn audio_settings_page(&mut self, ui: &mut egui::Ui, runtime: &RuntimeView) {
        let view = &runtime.audio_settings;
        ui.heading("Audio");
        ui.add_space(4.0);
        form::section_heading(ui, "Audio System");
        ui.add_space(4.0);

        let current = &view.current;
        if self.audio_draft.driver.is_empty() {
            self.audio_draft.driver.clone_from(&current.driver);
            self.audio_draft.device.clone_from(&current.device);
            self.audio_draft
                .sample_rate
                .clone_from(&current.sample_rate);
            if self.audio_draft.block_size == 0 {
                self.audio_draft.block_size = current.block_size.max(256);
            }
        }

        let driver_label = self.driver_label(view);
        let drivers = view
            .drivers
            .iter()
            .map(|driver| (driver.id.clone(), driver.label.clone()))
            .collect();
        if form::select_row(
            ui,
            "Driver",
            "audio-driver",
            &mut self.audio_draft.driver,
            driver_label,
            drivers,
        ) {
            self.audio_draft.device = view
                .drivers
                .iter()
                .find(|driver| driver.id == self.audio_draft.driver)
                .and_then(|driver| driver.devices.first())
                .map(|device| device.id.clone())
                .unwrap_or_default();
            self.audio_draft.sample_rate = "auto".into();
        }

        let device_label = self.device_label(view);
        let devices = self
            .devices_for(view)
            .iter()
            .map(|device| (device.id.clone(), device.label.clone()))
            .collect();
        if form::select_row(
            ui,
            "Device",
            "audio-device",
            &mut self.audio_draft.device,
            device_label,
            devices,
        ) {
            self.audio_draft.sample_rate = "auto".into();
        }

        let rate_label = self.sample_rate_label(view);
        let mut rates = vec![("auto".to_string(), "Automatic".to_string())];
        rates.extend(
            self.sample_rates_for(view)
                .into_iter()
                .map(|rate| (rate.to_string(), format!("{rate} Hz"))),
        );
        form::select_row(
            ui,
            "Sample rate",
            "audio-rate",
            &mut self.audio_draft.sample_rate,
            rate_label,
            rates,
        );

        ui.horizontal(|ui| {
            ui.label(RichText::new("Block size").size(13.0));
            if self.audio_draft.block_size == 0 {
                self.audio_draft.block_size = 512;
            }
            ui.add(
                egui::Slider::new(&mut self.audio_draft.block_size, 16..=2048)
                    .step_by(16.0)
                    .show_value(false),
            );
            let rate = self
                .audio_draft
                .sample_rate
                .parse::<u32>()
                .ok()
                .or_else(|| current.sample_rate.parse::<u32>().ok())
                .unwrap_or(48_000);
            let latency_ms = f64::from(self.audio_draft.block_size) / f64::from(rate) * 1000.0;
            ui.label(
                RichText::new(format!(
                    "{} samples / {:.2} ms",
                    self.audio_draft.block_size, latency_ms
                ))
                .monospace()
                .color(theme::ACCENT),
            );
        });
        ui.add_space(14.0);

        ui.horizontal(|ui| {
            if ui.button("Apply").clicked() {
                let sample_rate = self.audio_draft.sample_rate.parse::<u32>().ok();
                let driver = if self.audio_draft.driver.is_empty() {
                    current.driver.clone()
                } else {
                    self.audio_draft.driver.clone()
                };
                let device = if self.audio_draft.device.is_empty() {
                    current.device.clone()
                } else {
                    self.audio_draft.device.clone()
                };
                self.emit(UiIntent::ApplyAudioSettings {
                    driver,
                    device,
                    sample_rate,
                    block_size: self.audio_draft.block_size,
                });
            }
            if ui.button("Revert").clicked() {
                self.audio_draft = current.clone();
            }
        });

        if let Some(error) = &view.error {
            ui.add_space(10.0);
            ui.label(RichText::new(error).color(Color32::from_rgb(240, 120, 110)));
        }
        if let Some(warning) = &view.warning {
            ui.add_space(10.0);
            ui.label(RichText::new(warning).color(theme::ACCENT));
        }

        ui.add_space(18.0);
        form::section_heading(ui, "Outputs");
        egui::Frame::new()
            .fill(Color32::from_rgb(18, 19, 23))
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Stereo Left / Right").monospace());
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new("Main output")
                            .small()
                            .color(theme::TEXT_MUTED),
                    );
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Mono").monospace());
                    ui.add_space(20.0);
                    ui.label(
                        RichText::new("Not configured")
                            .small()
                            .color(theme::TEXT_MUTED),
                    );
                });
            });
    }

    fn driver_label(&self, view: &crate::runtime::AudioSettingsView) -> String {
        view.drivers
            .iter()
            .find(|driver| driver.id == self.audio_draft.driver)
            .map_or_else(
                || {
                    if self.audio_draft.driver.is_empty() {
                        "Select a driver".into()
                    } else {
                        self.audio_draft.driver.clone()
                    }
                },
                |driver| driver.label.clone(),
            )
    }

    fn device_label(&self, view: &crate::runtime::AudioSettingsView) -> String {
        self.devices_for(view)
            .iter()
            .find(|device| device.id == self.audio_draft.device)
            .map_or_else(
                || {
                    if self.audio_draft.device.is_empty() {
                        "Select a device".into()
                    } else {
                        self.audio_draft.device.clone()
                    }
                },
                |device| device.label.clone(),
            )
    }

    fn sample_rate_label(&self, _view: &crate::runtime::AudioSettingsView) -> String {
        if self.audio_draft.sample_rate == "auto" || self.audio_draft.sample_rate.is_empty() {
            "Automatic".into()
        } else {
            format!("{} Hz", self.audio_draft.sample_rate)
        }
    }

    fn devices_for<'a>(
        &self,
        view: &'a crate::runtime::AudioSettingsView,
    ) -> &'a [crate::runtime::AudioDeviceView] {
        view.drivers
            .iter()
            .find(|driver| driver.id == self.audio_draft.driver)
            .map_or(&[], |driver| driver.devices.as_slice())
    }

    fn sample_rates_for(&self, view: &crate::runtime::AudioSettingsView) -> Vec<u32> {
        self.devices_for(view)
            .iter()
            .find(|device| device.id == self.audio_draft.device)
            .map_or_else(Vec::new, |device| device.sample_rates.clone())
    }
}
