use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Stroke, Vec2};
use notwig_core::{ClipPayload, DeviceId, DeviceInstance, ParamValue, ProjectDoc, TrackId};

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
            .resizable(false)
            .exact_width(250.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL_LIGHT)
                    .inner_margin(egui::Margin {
                        left: 6,
                        right: 0,
                        top: 8,
                        bottom: 8,
                    }),
            )
            .show(ctx, |ui| {
                let height = ui.available_height();
                let content_width = (ui.available_width() - 10.0).max(120.0);
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(content_width, height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            let track_only = self.state.selection.primary_device().is_none()
                                && self.state.selection.primary_clip().is_none()
                                && self.state.selection.time.is_none();
                            if track_only
                                && let Some(track_id) = self.state.selection.primary_track()
                            {
                                self.track_info(ui, doc, runtime, track_id);
                            } else {
                                egui::ScrollArea::vertical()
                                    .id_salt("info-content")
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        if let Some(device_id) =
                                            self.state.selection.primary_device()
                                        {
                                            self.device_info(ui, doc, device_id);
                                        } else if let Some(clip_id) =
                                            self.state.selection.primary_clip()
                                        {
                                            self.clip_info(ui, doc, clip_id);
                                        } else if let Some(time) = self.state.selection.time.clone()
                                        {
                                            self.time_info(ui, doc, &time);
                                        } else {
                                            ui.label(
                                                RichText::new(
                                                    "No selection. Click a track, clip, or device.",
                                                )
                                                .color(theme::TEXT_MUTED),
                                            );
                                        }
                                    });
                            }
                        },
                    );
                    if crate::widgets::collapse_handle(
                        ui,
                        Vec2::new(10.0, height),
                        true,
                        "info-panel",
                        "Drag left to collapse",
                    ) {
                        self.show_info = false;
                    }
                });
            });
    }

    fn track_color_editor(&mut self, ui: &mut egui::Ui, track: &notwig_core::tracks::Track) {
        ui.add_space(6.0);
        ui.spacing_mut().item_spacing = Vec2::new(2.0, 2.0);
        ui.horizontal_wrapped(|ui| {
            for color in TRACK_COLORS {
                let color = notwig_core::Color {
                    r: color[0],
                    g: color[1],
                    b: color[2],
                    a: 255,
                };
                let (rect, response) =
                    ui.allocate_exact_size(Vec2::new(18.0, 16.0), Sense::click());
                let fill = Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a);
                ui.painter()
                    .rect_filled(rect, egui::CornerRadius::same(2), fill);
                if track.color == Some(color) {
                    ui.painter().rect_stroke(
                        rect,
                        egui::CornerRadius::same(2),
                        Stroke::new(2.0_f32, Color32::WHITE),
                        egui::StrokeKind::Inside,
                    );
                }
                if response.clicked() {
                    self.emit(UiIntent::SetTrackColor {
                        track_id: track.id,
                        color: Some(color),
                    });
                }
            }
            let (rect, response) = ui.allocate_exact_size(Vec2::new(18.0, 16.0), Sense::click());
            ui.painter().rect_filled(
                rect,
                egui::CornerRadius::same(2),
                Color32::from_rgb(22, 24, 29),
            );
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                "×",
                FontId::proportional(12.0),
                theme::TEXT_MUTED,
            );
            if response.clicked() {
                self.emit(UiIntent::SetTrackColor {
                    track_id: track.id,
                    color: None,
                });
            }
        });
        ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
    }

    fn track_comment_editor(&mut self, ui: &mut egui::Ui, track: &notwig_core::tracks::Track) {
        ui.add_space(6.0);
        let mut comment = track.comment.clone();
        let response = ui.add(
            egui::TextEdit::multiline(&mut comment)
                .hint_text("Add a note for this track")
                .desired_rows(4)
                .desired_width(f32::INFINITY),
        );
        if response.changed() {
            self.emit(UiIntent::SetTrackComment {
                track_id: track.id,
                comment,
            });
        }
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
            ui.label(
                RichText::new(track_kind_title(track.kind))
                    .small()
                    .strong()
                    .monospace()
                    .color(if track.active {
                        theme::TEXT_MUTED
                    } else {
                        Color32::from_rgb(104, 108, 120)
                    }),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if crate::icons::icon_button_sized(
                    ui,
                    crate::icons::Icon::Power,
                    Vec2::new(26.0, 22.0),
                    track.active,
                    if track.active {
                        "Deactivate track"
                    } else {
                        "Activate track"
                    },
                )
                .clicked()
                {
                    self.emit(UiIntent::SetTrackActive {
                        track_id,
                        active: !track.active,
                    });
                }
            });
        });
        let mut name = track.name.clone();
        let response = ui.add(
            egui::TextEdit::singleline(&mut name)
                .hint_text("Track name")
                .desired_width(f32::INFINITY),
        );
        if response.changed() && !name.trim().is_empty() {
            self.emit(UiIntent::RenameTrack { track_id, name });
        }
        self.track_color_editor(ui, &track);
        self.track_comment_editor(ui, &track);
        ui.separator();

        let devices = doc
            .chains
            .get(&track.main_chain)
            .map(|chain| chain.devices.clone())
            .unwrap_or_default();
        let device_height = (ui.available_height() - 310.0).max(80.0);
        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), device_height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.label(
                    RichText::new("DEVICES")
                        .small()
                        .strong()
                        .color(theme::TEXT_MUTED),
                );
                egui::ScrollArea::vertical()
                    .id_salt(("track-devices", track_id))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
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
                                        let additive =
                                            ui.input(|i| i.modifiers.command || i.modifiers.shift);
                                        self.state
                                            .selection
                                            .select_device(device_id, track_id, additive);
                                    }
                                });
                            }
                        }
                    });
            },
        );

        ui.add_space(8.0);
        let available = ui.available_width().max(160.0);
        let gap = 6.0;
        let column_width = ((available - gap) * 0.5).max(60.0);
        let controls_height = ui.available_height().max(280.0);
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(column_width, controls_height),
                egui::Layout::top_down(egui::Align::Min),
                |_ui| {},
            );
            ui.allocate_ui_with_layout(
                Vec2::new(column_width, controls_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.horizontal(|ui| {
                        info_record_button(ui);
                        if mixer_toggle(ui, "S", track.mixer.solo, theme::ACCENT) {
                            self.emit(UiIntent::SetTrackMixer {
                                track_id,
                                volume_db: None,
                                pan: None,
                                mute: None,
                                solo: Some(!track.mixer.solo),
                            });
                        }
                        if mixer_toggle(ui, "M", track.mixer.mute, theme::PLAYHEAD) {
                            self.emit(UiIntent::SetTrackMixer {
                                track_id,
                                volume_db: None,
                                pan: None,
                                mute: Some(!track.mixer.mute),
                                solo: None,
                            });
                        }
                    });
                    ui.add_space(4.0);
                    if let Some(pan) = pan_slider(ui, track.mixer.pan.0) {
                        self.emit(UiIntent::SetTrackMixer {
                            track_id,
                            volume_db: None,
                            pan: Some(notwig_core::Pan(pan)),
                            mute: None,
                            solo: None,
                        });
                    }
                    ui.add_space(4.0);
                    peak_label(ui, runtime, track_id);
                    if let Some(volume) =
                        volume_meter_fader(ui, runtime, track_id, track.mixer.volume_db.0)
                    {
                        self.emit(UiIntent::SetTrackMixer {
                            track_id,
                            volume_db: Some(notwig_core::Db(volume)),
                            pan: None,
                            mute: None,
                            solo: None,
                        });
                    }
                },
            );
        });
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
            let enabled = matches!(
                values.get(&notwig_dsp::stock::band_param(band, 0)),
                Some(ParamValue::Bool(true))
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

const TRACK_COLORS: [[u8; 3]; 30] = [
    [86, 86, 88],
    [120, 121, 124],
    [164, 166, 170],
    [96, 128, 194],
    [116, 96, 176],
    [176, 108, 72],
    [184, 152, 88],
    [92, 164, 112],
    [214, 95, 91],
    [192, 74, 154],
    [224, 123, 64],
    [232, 168, 62],
    [212, 210, 86],
    [143, 203, 96],
    [86, 190, 146],
    [68, 188, 202],
    [82, 156, 226],
    [103, 124, 228],
    [143, 106, 226],
    [195, 100, 220],
    [250, 129, 140],
    [246, 146, 88],
    [244, 190, 89],
    [218, 222, 88],
    [162, 214, 96],
    [90, 204, 112],
    [77, 200, 160],
    [68, 198, 209],
    [81, 166, 232],
    [125, 139, 238],
];

fn track_kind_title(kind: notwig_core::TrackKind) -> &'static str {
    match kind {
        notwig_core::TrackKind::Audio => "AUDIO TRACK",
        notwig_core::TrackKind::Instrument => "INSTRUMENT TRACK",
        notwig_core::TrackKind::Group => "GROUP TRACK",
        notwig_core::TrackKind::BusFx => "FX TRACK",
        notwig_core::TrackKind::Master => "MASTER TRACK",
    }
}

fn info_record_button(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(34.0, 24.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, egui::CornerRadius::same(3), theme::GRID);
    painter.circle_filled(rect.center(), 5.0, Color32::from_rgb(231, 91, 82));
    ui.interact(rect, egui::Id::new("sidebar-record"), Sense::click())
        .on_hover_text("Record (not implemented)");
}

fn mixer_toggle(ui: &mut egui::Ui, label: &str, active: bool, color: Color32) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(30.0, 22.0), Sense::click());
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        egui::CornerRadius::same(3),
        if active { color } else { theme::GRID },
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::monospace(10.0),
        if active {
            Color32::BLACK
        } else {
            theme::TEXT_MUTED
        },
    );
    response.clicked()
}

fn pan_slider(ui: &mut egui::Ui, pan: f32) -> Option<f32> {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().max(80.0), 18.0),
        Sense::click_and_drag(),
    );
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    let value = crate::widgets::value_drag(
        ui,
        &response,
        response.id,
        f64::from(pan),
        0.0,
        -1.0,
        1.0,
        0.01,
        false,
    )
    .map_or(pan, |value| value as f32);
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        egui::CornerRadius::same(3),
        Color32::from_rgb(15, 16, 20),
    );
    painter.rect_stroke(
        rect,
        egui::CornerRadius::same(3),
        Stroke::new(1.0_f32, theme::GRID),
        egui::StrokeKind::Inside,
    );
    let center = rect.center().x;
    let handle_x = rect.left() + rect.width() * (value + 1.0) * 0.5;
    let fill = Rect::from_min_max(
        egui::pos2(center.min(handle_x), rect.top()),
        egui::pos2(center.max(handle_x), rect.bottom()),
    );
    painter.rect_filled(
        fill,
        egui::CornerRadius::same(3),
        Color32::from_rgb(116, 67, 25),
    );
    painter.line_segment(
        [
            egui::pos2(center, rect.top() + 2.0),
            egui::pos2(center, rect.bottom() - 2.0),
        ],
        Stroke::new(1.0_f32, Color32::from_white_alpha(70)),
    );
    painter.line_segment(
        [
            egui::pos2(handle_x, rect.top() + 2.0),
            egui::pos2(handle_x, rect.bottom() - 2.0),
        ],
        Stroke::new(2.0_f32, Color32::from_rgb(255, 145, 46)),
    );
    ((value - pan).abs() > 1.0e-4).then_some(value)
}

fn peak_label(ui: &mut egui::Ui, runtime: &RuntimeView, track_id: TrackId) {
    let meter = runtime.meters.get(&track_id).copied().unwrap_or_default();
    let peak = meter.peak_l.max(meter.peak_r).max(1e-6);
    let db = 20.0 * peak.log10();
    let color = if db >= 0.0 {
        Color32::from_rgb(224, 80, 66)
    } else {
        theme::TEXT_MUTED
    };
    ui.allocate_ui_with_layout(
        Vec2::new(ui.available_width(), 14.0),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            ui.label(
                RichText::new(format!("{db:+.1}"))
                    .monospace()
                    .size(10.0)
                    .color(color),
            );
        },
    );
}

fn volume_meter_fader(
    ui: &mut egui::Ui,
    runtime: &RuntimeView,
    track_id: TrackId,
    volume_db: f32,
) -> Option<f32> {
    const DB_MIN: f32 = -60.0;
    const DB_MAX: f32 = 6.0;
    let height = 210.0;
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().max(140.0), height),
        Sense::hover(),
    );
    let meter = runtime.meters.get(&track_id).copied().unwrap_or_default();
    let painter = ui.painter().clone();
    let label_x = rect.left() + 4.0;
    for (db, label) in [
        (0.0_f32, "0"),
        (-6.0, "6"),
        (-12.0, "12"),
        (-18.0, "18"),
        (-24.0, "24"),
        (-30.0, "30"),
        (-36.0, "36"),
        (-48.0, "48"),
        (-60.0, "∞"),
    ] {
        let y = rect.top() + (-db / 60.0) * rect.height();
        painter.text(
            egui::pos2(label_x, y),
            Align2::LEFT_CENTER,
            label,
            FontId::monospace(10.0),
            theme::TEXT_MUTED,
        );
        painter.line_segment(
            [
                egui::pos2(rect.left() + 30.0, y),
                egui::pos2(rect.right() - 4.0, y),
            ],
            Stroke::new(1.0_f32, theme::GRID),
        );
    }
    let bars_left = rect.left() + 38.0;
    let bar_w = 11.0;
    let gap = 2.0;
    let meter_rect = Rect::from_min_max(
        egui::pos2(bars_left, rect.top()),
        egui::pos2(bars_left + bar_w * 2.0 + gap, rect.bottom()),
    );
    crate::widgets::draw_stereo_meter(&painter, meter_rect, meter.peak_l, meter.peak_r);
    let fader = Rect::from_min_max(
        egui::pos2(rect.right() - 26.0, rect.top()),
        egui::pos2(rect.right() - 6.0, rect.bottom()),
    );
    let response = ui.interact(
        fader.expand(4.0),
        egui::Id::new(("sidebar-volume", track_id)),
        Sense::click_and_drag(),
    );
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
    }
    let value = crate::widgets::value_drag(
        ui,
        &response,
        response.id,
        f64::from(volume_db),
        0.0,
        f64::from(DB_MIN),
        f64::from(DB_MAX),
        f64::from(DB_MAX - DB_MIN) / 200.0,
        true,
    )
    .map_or(volume_db, |value| value as f32);
    painter.rect_filled(
        fader,
        egui::CornerRadius::same(3),
        Color32::from_rgb(15, 16, 20),
    );
    painter.rect_stroke(
        fader,
        egui::CornerRadius::same(3),
        Stroke::new(1.0_f32, theme::GRID),
        egui::StrokeKind::Inside,
    );
    let t = ((volume_db - DB_MIN) / (DB_MAX - DB_MIN)).clamp(0.0, 1.0);
    let fill_top = fader.bottom() - fader.height() * t;
    painter.rect_filled(
        Rect::from_min_max(egui::pos2(fader.left(), fill_top), fader.max),
        egui::CornerRadius::same(3),
        Color32::from_rgb(116, 67, 25),
    );
    painter.line_segment(
        [
            egui::pos2(fader.left() + 2.0, fill_top),
            egui::pos2(fader.right() - 2.0, fill_top),
        ],
        Stroke::new(2.0_f32, Color32::from_rgb(255, 145, 46)),
    );
    painter.text(
        egui::pos2(rect.right() - 4.0, rect.top()),
        Align2::RIGHT_TOP,
        format_db(notwig_core::Db(volume_db)),
        FontId::monospace(10.0),
        theme::ACCENT,
    );
    ((value - volume_db).abs() > 1.0e-4).then_some(value)
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
