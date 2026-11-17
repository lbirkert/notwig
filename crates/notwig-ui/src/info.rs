use egui::{Color32, RichText, Vec2};
use notwig_core::{
    ClipPayload, DeviceId, DeviceInstance, ParamValue, ProjectDoc, TrackId, TrackOutput,
};

use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::shell::NotwigUi;
use crate::theme;
use crate::widgets::{format_beats, format_db, format_param};

impl NotwigUi {
    pub(crate) fn info_panel(
        &mut self,
        ctx: &egui::Context,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
    ) {
        if !self.show_info {
            return;
        }
        egui::SidePanel::left("notwig_info")
            .resizable(true)
            .default_width(270.0)
            .min_width(210.0)
            .max_width(380.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL_LIGHT)
                    .inner_margin(egui::Margin::same(8)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("INFO");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("×").clicked() {
                            self.show_info = false;
                        }
                    });
                });
                ui.separator();
                if let Some(device_id) = self.state.selection.primary_device() {
                    self.device_info(ui, doc, device_id);
                } else if let Some(clip_id) = self.state.selection.primary_clip() {
                    self.clip_info(ui, doc, clip_id);
                } else if let Some(time) = self.state.selection.time.clone() {
                    self.time_info(ui, doc, &time);
                } else if let Some(track_id) = self.state.selection.primary_track() {
                    self.track_info(ui, doc, runtime, track_id);
                } else {
                    ui.label(
                        RichText::new("No selection. Click a track, clip, or device.")
                            .color(theme::TEXT_MUTED),
                    );
                }
            });
    }

    fn track_info(
        &mut self,
        ui: &mut egui::Ui,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
        track_id: TrackId,
    ) {
        let Some(track) = doc.tracks.get(&track_id).cloned() else {
            return;
        };
        let count = self.state.selection.tracks.len();
        section_header(ui, if count > 1 { "TRACKS" } else { "TRACK" });
        if count > 1 {
            ui.label(
                RichText::new(format!("{count} tracks selected"))
                    .small()
                    .color(theme::TEXT_MUTED),
            );
            let selected: Vec<_> = self.state.selection.tracks.iter().copied().collect();
            for id in selected {
                if let Some(other) = doc.tracks.get(&id) {
                    if ui.selectable_label(id == track_id, &other.name).clicked() {
                        self.state.selection.tracks.set_primary(id);
                    }
                }
            }
            ui.separator();
        }
        ui.horizontal(|ui| {
            if let Some(color) = track.color {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::hover());
                ui.painter().rect_filled(
                    rect,
                    egui::CornerRadius::same(3),
                    Color32::from_rgba_unmultiplied(color.r, color.g, color.b, 255),
                );
            }
            ui.label(RichText::new(&track.name).strong().size(16.0));
        });
        ui.label(
            RichText::new(track.kind.name().to_uppercase())
                .small()
                .monospace()
                .color(theme::TEXT_MUTED),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let active = !track.mixer.mute;
            if crate::icons::icon_button_sized(
                ui,
                crate::icons::Icon::Power,
                Vec2::new(26.0, 22.0),
                active,
                if active { "Mute track" } else { "Unmute track" },
            )
            .clicked()
            {
                self.emit(UiIntent::SetTrackMixer {
                    track_id,
                    volume_db: None,
                    pan: None,
                    mute: Some(active),
                    solo: None,
                });
            }
            let mut solo = track.mixer.solo;
            if ui.toggle_value(&mut solo, "S").changed() {
                self.emit(UiIntent::SetTrackMixer {
                    track_id,
                    volume_db: None,
                    pan: None,
                    mute: None,
                    solo: Some(solo),
                });
            }
            ui.label(
                RichText::new(if active { "Active" } else { "Muted" })
                    .small()
                    .color(if active {
                        theme::ACCENT
                    } else {
                        theme::TEXT_MUTED
                    }),
            );
        });
        ui.add_space(6.0);
        let mut volume = track.mixer.volume_db.0;
        ui.label(RichText::new("Volume").small().color(theme::TEXT_MUTED));
        if ui
            .add(egui::Slider::new(&mut volume, -60.0..=6.0).show_value(false))
            .changed()
        {
            self.emit(UiIntent::SetTrackMixer {
                track_id,
                volume_db: Some(notwig_core::Db(volume)),
                pan: None,
                mute: None,
                solo: None,
            });
        }
        ui.label(
            RichText::new(format_db(notwig_core::Db(volume)))
                .small()
                .monospace()
                .color(theme::ACCENT),
        );
        let mut pan = track.mixer.pan.0;
        if ui
            .add(egui::Slider::new(&mut pan, -1.0..=1.0).show_value(false))
            .changed()
        {
            self.emit(UiIntent::SetTrackMixer {
                track_id,
                volume_db: None,
                pan: Some(notwig_core::Pan(pan)),
                mute: None,
                solo: None,
            });
        }
        ui.label(
            RichText::new(format!("Pan {pan:+.2}"))
                .small()
                .monospace()
                .color(theme::TEXT_MUTED),
        );
        ui.separator();

        ui.label(
            RichText::new("DEVICES")
                .small()
                .strong()
                .color(theme::TEXT_MUTED),
        );
        let devices = doc
            .chains
            .get(&track.main_chain)
            .map(|chain| chain.devices.clone())
            .unwrap_or_default();
        if devices.is_empty() {
            ui.label(
                RichText::new("No devices on this track.")
                    .small()
                    .color(theme::TEXT_MUTED),
            );
        }
        for device_id in devices {
            if let Some(device) = doc.devices.get(&device_id) {
                let selected = self.state.selection.devices.contains(&device_id);
                let name = device
                    .name
                    .clone()
                    .unwrap_or_else(|| self.device_name(device));
                ui.horizontal(|ui| {
                    if crate::icons::icon_button_sized(
                        ui,
                        crate::icons::Icon::Power,
                        Vec2::new(22.0, 20.0),
                        !device.bypass,
                        if device.bypass {
                            "Enable device"
                        } else {
                            "Bypass device"
                        },
                    )
                    .clicked()
                    {
                        self.emit(UiIntent::SetDeviceBypass {
                            device_id,
                            bypass: !device.bypass,
                        });
                    }
                    if ui.selectable_label(selected, name).clicked() {
                        let additive = ui.input(|i| i.modifiers.command || i.modifiers.shift);
                        self.state
                            .selection
                            .select_device(device_id, track_id, additive);
                    }
                });
            }
        }
        ui.separator();
        ui.label(
            RichText::new("ROUTING")
                .small()
                .strong()
                .color(theme::TEXT_MUTED),
        );
        let destination = match track.io.audio_out {
            TrackOutput::Track(id) => doc
                .tracks
                .get(&id)
                .map_or_else(|| id.to_string(), |track| track.name.clone()),
            TrackOutput::Default => track.parent.map_or_else(
                || "Master".into(),
                |parent| {
                    doc.tracks
                        .get(&parent)
                        .map_or_else(|| parent.to_string(), |track| track.name.clone())
                },
            ),
            TrackOutput::None => "None".into(),
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new("Output").small().color(theme::TEXT_MUTED));
            ui.label(destination);
        });
        if !track.sends.is_empty() {
            ui.label(
                RichText::new("SENDS")
                    .small()
                    .strong()
                    .color(theme::TEXT_MUTED),
            );
            for send in &track.sends {
                let target = doc
                    .tracks
                    .get(&send.target)
                    .map_or_else(|| send.target.to_string(), |track| track.name.clone());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(target).small());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format_db(send.amount_db))
                                .small()
                                .monospace()
                                .color(theme::TEXT_MUTED),
                        );
                    });
                });
            }
        }
        ui.separator();
        if let Some(meter) = runtime.meters.get(&track_id) {
            ui.label(
                RichText::new(format!(
                    "Peak {:+.1} / {:+.1} dB",
                    linear_db(meter.peak_l),
                    linear_db(meter.peak_r)
                ))
                .small()
                .monospace()
                .color(theme::TEXT_MUTED),
            );
        }
    }

    #[allow(clippy::unused_self)]
    fn time_info(
        &mut self,
        ui: &mut egui::Ui,
        doc: &ProjectDoc,
        time: &crate::state::TimeSelection,
    ) {
        section_header(ui, "TIME SELECTION");
        ui.horizontal(|ui| {
            ui.label(RichText::new("Start").small().color(theme::TEXT_MUTED));
            ui.label(format_beats(time.range.start, doc.meta.time_sig_num));
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("End").small().color(theme::TEXT_MUTED));
            ui.label(format_beats(time.range.end, doc.meta.time_sig_num));
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("Length").small().color(theme::TEXT_MUTED));
            ui.label(format_beats(time.range.length(), doc.meta.time_sig_num));
        });
        ui.separator();
        ui.label(
            RichText::new(format!("{} lane(s)", time.tracks.len()))
                .small()
                .color(theme::TEXT_MUTED),
        );
        let selected: Vec<_> = time.tracks.iter().copied().collect();
        for id in selected {
            if let Some(track) = doc.tracks.get(&id) {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&track.name).small());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(track.kind.name().to_uppercase())
                                .small()
                                .monospace()
                                .color(theme::TEXT_MUTED),
                        );
                    });
                });
            }
        }
    }

    fn device_info(&mut self, ui: &mut egui::Ui, doc: &ProjectDoc, device_id: DeviceId) {
        let Some(device) = doc.devices.get(&device_id).cloned() else {
            return;
        };
        let count = self.state.selection.devices.len();
        section_header(ui, if count > 1 { "DEVICES" } else { "DEVICE" });
        if count > 1 {
            ui.label(
                RichText::new(format!("{count} devices selected"))
                    .small()
                    .color(theme::TEXT_MUTED),
            );
            let selected: Vec<_> = self.state.selection.devices.iter().copied().collect();
            for id in selected {
                if let Some(other) = doc.devices.get(&id) {
                    let name = other
                        .name
                        .clone()
                        .unwrap_or_else(|| self.device_name(other));
                    if ui.selectable_label(id == device_id, name).clicked() {
                        self.state.selection.devices.set_primary(id);
                    }
                }
            }
            ui.separator();
        }
        ui.horizontal(|ui| {
            if crate::icons::icon_button_sized(
                ui,
                crate::icons::Icon::Power,
                Vec2::new(26.0, 22.0),
                !device.bypass,
                if device.bypass {
                    "Enable device"
                } else {
                    "Bypass device"
                },
            )
            .clicked()
            {
                self.emit(UiIntent::SetDeviceBypass {
                    device_id,
                    bypass: !device.bypass,
                });
            }
            ui.vertical(|ui| {
                ui.label(RichText::new(self.device_name(&device)).strong().size(15.0));
                ui.label(
                    RichText::new(device.type_id.clone())
                        .small()
                        .monospace()
                        .color(theme::TEXT_MUTED),
                );
            });
        });
        ui.label(
            RichText::new(if device.bypass { "Bypassed" } else { "Active" })
                .small()
                .color(if device.bypass {
                    theme::TEXT_MUTED
                } else {
                    theme::ACCENT
                }),
        );
        ui.separator();
        if device.type_id == notwig_dsp::stock::EQ_TYPE_ID {
            self.eq_info(ui, &device);
        } else {
            self.generic_device_info(ui, &device);
        }
    }

    fn eq_info(&mut self, ui: &mut egui::Ui, device: &DeviceInstance) {
        let values = self.device_values(device);
        ui.label(
            RichText::new("BANDS")
                .small()
                .strong()
                .color(theme::TEXT_MUTED),
        );
        for band in 0..4 {
            let enabled = !matches!(
                values.get(&notwig_dsp::stock::band_param(band, 0)),
                Some(ParamValue::Bool(false))
            );
            let kind = values
                .get(&notwig_dsp::stock::band_param(band, 1))
                .and_then(ParamValue::as_f64)
                .unwrap_or(0.0) as u32;
            let freq = values
                .get(&notwig_dsp::stock::band_param(band, 2))
                .and_then(ParamValue::as_f64)
                .unwrap_or(1_000.0);
            let gain = values
                .get(&notwig_dsp::stock::band_param(band, 3))
                .and_then(ParamValue::as_f64)
                .unwrap_or(0.0);
            let q = values
                .get(&notwig_dsp::stock::band_param(band, 4))
                .and_then(ParamValue::as_f64)
                .unwrap_or(0.707);
            let selected = self.eq_band.min(3) == band;
            egui::Frame::new()
                .fill(if selected {
                    theme::ACCENT_DIM
                } else {
                    theme::PANEL
                })
                .inner_margin(egui::Margin::symmetric(5, 4))
                .corner_radius(egui::CornerRadius::same(4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if crate::icons::icon_button_sized(
                            ui,
                            crate::icons::Icon::Power,
                            Vec2::new(20.0, 18.0),
                            enabled,
                            if enabled {
                                "Disable band"
                            } else {
                                "Enable band"
                            },
                        )
                        .clicked()
                        {
                            self.emit(UiIntent::SetParam {
                                device_id: device.id,
                                param_id: notwig_dsp::stock::band_param(band, 0),
                                value: ParamValue::Bool(!enabled),
                            });
                        }
                        if ui
                            .selectable_label(
                                selected,
                                format!("{}  {}", band + 1, band_type_label(kind)),
                            )
                            .clicked()
                        {
                            self.eq_band = band;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!("{freq:.0} Hz · {gain:+.1} dB · {q:.2}"))
                                    .small()
                                    .monospace()
                                    .color(theme::TEXT_MUTED),
                            );
                        });
                    });
                });
            ui.add_space(2.0);
        }
        ui.separator();
        egui::CollapsingHeader::new("Spectrum Analyzer")
            .default_open(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.analyzer_pre, "Pre");
                    ui.checkbox(&mut self.analyzer_post, "Post");
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Resolution").small());
                    egui::ComboBox::from_id_salt("analyzer-resolution")
                        .selected_text(["Low", "Medium", "High"][self.analyzer_resolution.min(2)])
                        .show_ui(ui, |ui| {
                            for (index, label) in ["Low", "Medium", "High"].iter().enumerate() {
                                ui.selectable_value(&mut self.analyzer_resolution, index, *label);
                            }
                        });
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Speed").small());
                    egui::ComboBox::from_id_salt("analyzer-speed")
                        .selected_text(["Slow", "Medium", "Fast"][self.analyzer_speed.min(2)])
                        .show_ui(ui, |ui| {
                            for (index, label) in ["Slow", "Medium", "Fast"].iter().enumerate() {
                                ui.selectable_value(&mut self.analyzer_speed, index, *label);
                            }
                        });
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Tilt").small());
                    ui.add(
                        egui::DragValue::new(&mut self.analyzer_tilt)
                            .speed(0.1)
                            .suffix(" dB/oct"),
                    );
                });
                ui.label(
                    RichText::new(format!(
                        "Range {:.0} … {:.0} dB",
                        self.analyzer_range.0, self.analyzer_range.1
                    ))
                    .small()
                    .color(theme::TEXT_MUTED),
                );
            });
    }

    fn generic_device_info(&mut self, ui: &mut egui::Ui, device: &DeviceInstance) {
        let Some(spec) = self.registry.resolve(&device.type_id, device.version) else {
            ui.label(
                RichText::new("No parameter metadata available.")
                    .small()
                    .color(theme::TEXT_MUTED),
            );
            return;
        };
        ui.label(RichText::new(&spec.name).strong().color(theme::TEXT_MUTED));
        for param in spec.params.iter().take(12) {
            let value = device
                .param_values
                .get(&param.id)
                .cloned()
                .unwrap_or_else(|| param.default.clone());
            let physical = param.physical(&value);
            ui.horizontal(|ui| {
                ui.label(RichText::new(&param.name).small());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format_param(param.unit, physical))
                            .small()
                            .monospace()
                            .color(theme::ACCENT),
                    );
                });
            });
        }
    }

    fn clip_info(&mut self, ui: &mut egui::Ui, doc: &ProjectDoc, clip_id: notwig_core::ClipId) {
        let Some(clip) = doc.arranger.clips.get(&clip_id).cloned() else {
            return;
        };
        let count = self.state.selection.clips.len();
        section_header(ui, if count > 1 { "CLIPS" } else { "CLIP" });
        if count > 1 {
            ui.label(
                RichText::new(format!("{count} clips selected"))
                    .small()
                    .color(theme::TEXT_MUTED),
            );
            ui.separator();
        }
        let name = clip.name.clone().unwrap_or_else(|| "Clip".into());
        ui.label(RichText::new(name).strong().size(15.0));
        ui.label(
            RichText::new(match &clip.payload {
                ClipPayload::Midi(_) => "MIDI CLIP",
                ClipPayload::Audio(_) => "AUDIO CLIP",
            })
            .small()
            .monospace()
            .color(theme::TEXT_MUTED),
        );
        ui.horizontal(|ui| {
            ui.label(RichText::new("Track").small().color(theme::TEXT_MUTED));
            ui.label(
                doc.tracks
                    .get(&clip.track_id)
                    .map_or("Unknown", |track| track.name.as_str()),
            );
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("Start").small().color(theme::TEXT_MUTED));
            ui.label(format_beats(clip.range.start, doc.meta.time_sig_num));
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("Length").small().color(theme::TEXT_MUTED));
            ui.label(format_beats(clip.range.length(), doc.meta.time_sig_num));
        });
        match &clip.payload {
            ClipPayload::Midi(midi) => {
                ui.label(
                    RichText::new(format!("{} notes", midi.notes.len()))
                        .small()
                        .color(theme::TEXT_MUTED),
                );
            }
            ClipPayload::Audio(audio) => {
                ui.label(
                    RichText::new(format!(
                        "{:.2} s · fades {:.2} / {:.2} s",
                        audio.source_duration() as f64 / f64::from(doc.meta.sample_rate.value()),
                        audio.fade_in_samples as f64 / f64::from(doc.meta.sample_rate.value()),
                        audio.fade_out_samples as f64 / f64::from(doc.meta.sample_rate.value()),
                    ))
                    .small()
                    .color(theme::TEXT_MUTED),
                );
            }
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let mut muted = clip.muted;
            if ui.toggle_value(&mut muted, "Muted").changed() {
                self.emit(UiIntent::SetClipMuted { clip_id, muted });
            }
            if ui.button("Duplicate").clicked() {
                self.emit(UiIntent::DuplicateClip { clip_id });
            }
            if ui.button("Delete").clicked() {
                self.emit(UiIntent::DeleteClip(clip_id));
            }
        });
    }

    pub(crate) fn device_name(&self, device: &DeviceInstance) -> String {
        device.name.clone().unwrap_or_else(|| {
            self.registry
                .resolve(&device.type_id, device.version)
                .map_or_else(|| device.type_id.clone(), |spec| spec.name.clone())
        })
    }
}

fn section_header(ui: &mut egui::Ui, label: &str) {
    ui.label(
        RichText::new(label)
            .small()
            .monospace()
            .strong()
            .color(theme::TEXT_MUTED),
    );
}

fn band_type_label(index: u32) -> &'static str {
    match index {
        1 => "Low Shelf",
        2 => "High Shelf",
        3 => "High Pass",
        4 => "Low Pass",
        _ => "Bell",
    }
}

fn linear_db(value: f32) -> f32 {
    if value <= 1.0e-6 {
        -120.0
    } else {
        20.0 * value.log10()
    }
}
