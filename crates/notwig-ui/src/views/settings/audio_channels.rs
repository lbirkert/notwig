use egui::RichText;

use crate::form;
use crate::runtime::{AudioChannelConfig, AudioChannelDirection, AudioChannelFormat, RuntimeView};
use crate::theme;

use super::NotwigUi;

impl NotwigUi {
    pub(super) fn audio_io_settings(&mut self, ui: &mut egui::Ui, runtime: &RuntimeView) {
        form::section_heading(ui, "Inputs & Outputs");
        let selected = selected_device(runtime, &self.audio_draft);
        let input_channels = selected.map_or(0, |device| device.input_channels);
        let output_channels = selected.map_or(0, |device| device.output_channels);
        let device_label = selected.map_or_else(
            || "the selected audio device".to_string(),
            |device| device.label.clone(),
        );
        ui.label(
            RichText::new(format!(
                "Name channels on {device_label}. Named inputs appear in the track input picker for recording."
            ))
            .small()
            .color(theme::TEXT_MUTED),
        );
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            for (direction, format, available) in [
                (
                    AudioChannelDirection::Input,
                    AudioChannelFormat::Mono,
                    input_channels,
                ),
                (
                    AudioChannelDirection::Input,
                    AudioChannelFormat::Stereo,
                    input_channels,
                ),
                (
                    AudioChannelDirection::Output,
                    AudioChannelFormat::Mono,
                    output_channels,
                ),
                (
                    AudioChannelDirection::Output,
                    AudioChannelFormat::Stereo,
                    output_channels,
                ),
            ] {
                let label = format!(
                    "Add {} {}",
                    format.label().to_lowercase(),
                    direction.label().to_lowercase()
                );
                let enabled = available >= format.channel_count();
                if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                    add_audio_channel(&mut self.audio_draft, direction, format, available);
                }
            }
        });
        ui.add_space(8.0);

        let mut channels = self.audio_draft.channels.clone();
        for channel in &mut channels {
            channel.driver.clone_from(&self.audio_draft.driver);
            channel.device.clone_from(&self.audio_draft.device);
            let available = if channel.direction == AudioChannelDirection::Input {
                input_channels
            } else {
                output_channels
            };
            normalize_channel_channels(channel, available);
        }
        if channels.is_empty() {
            ui.label(
                RichText::new("No named channels yet.")
                    .small()
                    .color(theme::TEXT_MUTED),
            );
        } else {
            let mut remove = None;
            for channel in &mut channels {
                let available = if channel.direction == AudioChannelDirection::Input {
                    input_channels
                } else {
                    output_channels
                };
                normalize_channel_channels(channel, available);
                egui::Frame::new()
                    .fill(theme::PANEL_LIGHT)
                    .stroke(egui::Stroke::new(1.0_f32, theme::GRID))
                    .corner_radius(egui::CornerRadius::same(7))
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut channel.name)
                                    .hint_text("Channel name")
                                    .desired_width(150.0),
                            );
                            ui.label(
                                RichText::new(channel.kind_label())
                                    .small()
                                    .color(theme::TEXT_MUTED),
                            );
                            if channel.channel_count() >= 2 {
                                ui.label(RichText::new("L").small().color(theme::TEXT_MUTED));
                                channel_channel_combo(
                                    ui,
                                    ("audio-channel-left", &channel.id),
                                    &mut channel.left_channel,
                                    available,
                                    None,
                                );
                                ui.label(RichText::new("R").small().color(theme::TEXT_MUTED));
                                let mut right = channel
                                    .right_channel
                                    .unwrap_or_else(|| channel.left_channel.saturating_add(1));
                                channel_channel_combo(
                                    ui,
                                    ("audio-channel-right", &channel.id),
                                    &mut right,
                                    available,
                                    Some(channel.left_channel),
                                );
                                channel.right_channel = Some(right);
                            } else {
                                channel_channel_combo(
                                    ui,
                                    ("audio-channel-left", &channel.id),
                                    &mut channel.left_channel,
                                    available,
                                    None,
                                );
                                channel.right_channel = None;
                            }
                            if ui.button("Remove").clicked() {
                                remove = Some(channel.id.clone());
                            }
                        });
                    });
                ui.add_space(4.0);
            }
            if let Some(id) = remove {
                channels.retain(|channel| channel.id != id);
                if self.audio_draft.master_output.as_deref() == Some(id.as_str()) {
                    self.audio_draft.master_output = None;
                }
            }
        }
        self.audio_draft.channels = channels;

        let output_channels: Vec<(String, String)> = self
            .audio_draft
            .channels
            .iter()
            .filter(|channel| channel.direction == AudioChannelDirection::Output)
            .map(|channel| (channel.id.clone(), channel.name.clone()))
            .collect();
        if !output_channels.is_empty() {
            ui.add_space(4.0);
            let mut master = self.audio_draft.master_output.clone();
            let selected = master
                .as_ref()
                .and_then(|id| {
                    output_channels
                        .iter()
                        .find(|(channel_id, _)| channel_id == id)
                        .map(|(_, name)| name.clone())
                })
                .unwrap_or_else(|| "Default output".to_string());
            ui.horizontal(|ui| {
                ui.label(RichText::new("Master output").size(13.0));
                form::combo(ui, "audio-master-output", selected, 220.0, |ui| {
                    if ui
                        .selectable_label(master.is_none(), "Default output")
                        .clicked()
                    {
                        master = None;
                    }
                    for (id, name) in &output_channels {
                        if ui
                            .selectable_label(master.as_deref() == Some(id.as_str()), name)
                            .clicked()
                        {
                            master = Some(id.clone());
                        }
                    }
                });
            });
            self.audio_draft.master_output = master;
        }
    }
}

fn selected_device<'a>(
    runtime: &'a RuntimeView,
    draft: &crate::runtime::AudioSettingsValue,
) -> Option<&'a crate::runtime::AudioDeviceView> {
    runtime
        .audio_settings
        .drivers
        .iter()
        .find(|driver| driver.id == draft.driver)
        .and_then(|driver| {
            driver
                .devices
                .iter()
                .find(|device| device.id == draft.device)
        })
}

fn normalize_channel_channels(channel: &mut AudioChannelConfig, available: u16) {
    if available == 0 {
        return;
    }
    channel.left_channel = channel.left_channel.min(available - 1);
    if channel.channel_count() >= 2 {
        if available < 2 {
            channel.format = AudioChannelFormat::Mono;
            channel.right_channel = None;
            return;
        }
        let mut right = channel
            .right_channel
            .unwrap_or_else(|| channel.left_channel.saturating_add(1))
            .min(available - 1);
        if right == channel.left_channel {
            right = (channel.left_channel + 1) % available;
        }
        channel.right_channel = Some(right);
    } else {
        channel.right_channel = None;
    }
}

fn channel_channel_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    channel: &mut u16,
    available: u16,
    exclude: Option<u16>,
) {
    let available = available.max(1);
    if *channel >= available {
        *channel = 0;
    }
    if exclude == Some(*channel) {
        *channel = (0..available)
            .find(|candidate| Some(*candidate) != exclude)
            .unwrap_or(*channel);
    }
    form::combo(ui, id, format!("Ch {}", *channel + 1), 72.0, |ui| {
        for candidate in 0..available {
            if Some(candidate) == exclude {
                continue;
            }
            if ui
                .selectable_label(candidate == *channel, format!("Ch {}", candidate + 1))
                .clicked()
            {
                *channel = candidate;
            }
        }
    });
}

fn add_audio_channel(
    draft: &mut crate::runtime::AudioSettingsValue,
    direction: AudioChannelDirection,
    format: AudioChannelFormat,
    available: u16,
) {
    if available < format.channel_count() {
        return;
    }
    let used = draft
        .channels
        .iter()
        .filter(|channel| channel.direction == direction)
        .map(|channel| {
            let right = channel.right_channel.unwrap_or(channel.left_channel);
            channel.left_channel.max(right).saturating_add(1)
        })
        .max()
        .unwrap_or(0);
    let left_channel = used.min(available.saturating_sub(format.channel_count()));
    let right_channel = (format.channel_count() >= 2).then(|| left_channel + 1);
    let count = draft
        .channels
        .iter()
        .filter(|channel| channel.direction == direction && channel.format == format)
        .count()
        + 1;
    draft.channels.push(AudioChannelConfig {
        id: format!("channel-{}", uuid::Uuid::new_v4()),
        name: format!("{} {} {count}", format.label(), direction.label()),
        direction,
        format,
        driver: draft.driver.clone(),
        device: draft.device.clone(),
        left_channel,
        right_channel,
    });
}
