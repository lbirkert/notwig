use egui::{Color32, RichText};

use crate::form;
use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::theme;

use super::NotwigUi;

impl NotwigUi {
    pub(super) fn audio_settings_page(&mut self, ui: &mut egui::Ui, runtime: &RuntimeView) {
        let view = &runtime.audio_settings;
        ui.label(theme::page_heading(ui.ctx(), "Audio"));
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
            self.audio_draft.channels.clone_from(&current.channels);
            self.audio_draft
                .master_output
                .clone_from(&current.master_output);
            self.audio_draft.low_latency_block_size = current.low_latency_block_size;
            self.audio_draft.low_latency_monitoring = current.low_latency_monitoring;
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
            .map(|device| {
                (
                    device.id.clone(),
                    format!(
                        "{} · {}-in, {}-out",
                        device.label, device.input_channels, device.output_channels
                    ),
                )
            })
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

        if self.audio_draft.block_size == 0 {
            self.audio_draft.block_size = 512;
        }
        if self.audio_draft.low_latency_block_size == 0 {
            self.audio_draft.low_latency_block_size = 16;
        }
        self.audio_draft.low_latency_block_size = self
            .audio_draft
            .low_latency_block_size
            .clamp(16, self.audio_draft.block_size);
        let rate = self
            .audio_draft
            .sample_rate
            .parse::<u32>()
            .ok()
            .or_else(|| current.sample_rate.parse::<u32>().ok())
            .unwrap_or(48_000);
        let latency_ms = |samples: u32| f64::from(samples) / f64::from(rate) * 1000.0;
        let full_width = ui.max_rect().width().max(ui.available_width());
        ui.checkbox(
            &mut self.audio_draft.low_latency_monitoring,
            "Render the project in a higher block size than monitoring",
        );
        // Ordering is load-bearing: clamp `block_size` first, then derive
        // `device_max`, then clamp the device buffer against it. While
        // low-latency monitoring is on the device buffer may not exceed the
        // render block size; toggling the checkbox changes `device_max` and
        // immediately re-clamps the device buffer.
        self.audio_draft.block_size = self
            .audio_draft
            .block_size
            .clamp(16, crate::MAX_AUDIO_BLOCK_SIZE);
        let device_max = if self.audio_draft.low_latency_monitoring {
            self.audio_draft.block_size
        } else {
            crate::MAX_AUDIO_BLOCK_SIZE_WITHOUT_LOW_LATENCY
        };
        self.audio_draft.low_latency_block_size = self
            .audio_draft
            .low_latency_block_size
            .clamp(16, device_max);

        ui.label(RichText::new("Device Block Size").size(13.0));
        styled_buffer_slider(
            ui,
            &mut self.audio_draft.low_latency_block_size,
            device_max,
            full_width,
        );
        ui.label(
            RichText::new(format!(
                "{} samples / {:.2} ms",
                self.audio_draft.low_latency_block_size,
                latency_ms(self.audio_draft.low_latency_block_size)
            ))
            .monospace()
            .color(theme::ACCENT),
        );
        ui.add_space(8.0);
        ui.add_enabled_ui(self.audio_draft.low_latency_monitoring, |ui| {
            ui.label(RichText::new("High latency buffer").size(13.0));
            styled_buffer_slider(
                ui,
                &mut self.audio_draft.block_size,
                crate::MAX_AUDIO_BLOCK_SIZE,
                full_width,
            );
            ui.label(
                RichText::new(format!(
                    "{} samples / {:.2} ms",
                    self.audio_draft.block_size,
                    latency_ms(self.audio_draft.block_size)
                ))
                .monospace()
                .color(theme::ACCENT),
            );
        });
        ui.add_space(14.0);
        self.audio_io_settings(ui, runtime);
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
                    low_latency_block_size: self.audio_draft.low_latency_block_size,
                    low_latency_monitoring: self.audio_draft.low_latency_monitoring,
                    channels: self.audio_draft.channels.clone(),
                    master_output: self.audio_draft.master_output.clone(),
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

fn styled_buffer_slider(ui: &mut egui::Ui, value: &mut u32, max: u32, full_width: f32) {
    ui.scope(|ui| {
        ui.spacing_mut().slider_width = full_width;
        ui.visuals_mut().selection.bg_fill = theme::ACCENT;
        ui.visuals_mut().widgets.inactive.bg_fill = theme::GRID_STRONG;
        ui.add_sized(
            [full_width, ui.spacing().interact_size.y.max(20.0)],
            egui::Slider::new(value, 16..=max)
                .step_by(16.0)
                .show_value(false)
                .trailing_fill(true),
        );
    });
}
