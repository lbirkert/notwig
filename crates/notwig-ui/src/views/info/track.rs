use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Stroke, Vec2};
use notwig_core::tracks::Track;
use notwig_core::{Color, Db, Pan, ProjectDoc, TrackId, TrackKind};

use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::shell::NotwigUi;
use crate::theme;
use crate::widgets::format_db;

use super::common::{chain_latency, latency_label};

impl NotwigUi {
    fn track_color_editor(&mut self, ui: &mut egui::Ui, track: &Track) {
        ui.add_space(6.0);
        ui.spacing_mut().item_spacing = Vec2::new(2.0, 2.0);
        ui.horizontal_wrapped(|ui| {
            for color in TRACK_COLORS {
                let color = Color {
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

    fn track_input_editor(&mut self, ui: &mut egui::Ui, track: &Track, runtime: &RuntimeView) {
        let selected = track.io.audio_input.clone();
        let named_inputs: Vec<(String, notwig_core::AudioInputRef)> = runtime
            .audio_settings
            .current
            .channels
            .iter()
            .filter(|channel| channel.direction == crate::runtime::AudioChannelDirection::Input)
            .map(|channel| {
                (
                    channel.name.clone(),
                    notwig_core::AudioInputRef {
                        driver: channel.driver.clone(),
                        device: channel.device.clone(),
                        channel: channel.left_channel,
                        channels: channel.channel_count(),
                        right_channel: channel.resolved_right_channel(),
                    },
                )
            })
            .collect();
        let label = selected.as_ref().map_or_else(
            || "No input".to_string(),
            |input| {
                if let Some((name, _)) = named_inputs.iter().find(|(_, named)| named == input) {
                    return name.clone();
                }
                "Input not configured".to_string()
            },
        );
        egui::ComboBox::from_id_salt(("track-input", track.id))
            .selected_text(label)
            .width(ui.available_width().min(280.0))
            .show_ui(ui, |ui| {
                if named_inputs.is_empty() {
                    ui.label("No inputs configured in Audio Settings.");
                }
                for (name, option) in &named_inputs {
                    let label = if option.channels >= 2 {
                        format!("{name} · Stereo")
                    } else {
                        format!("{name} · Mono")
                    };
                    if ui
                        .selectable_label(selected.as_ref() == Some(option), label)
                        .clicked()
                    {
                        self.emit(UiIntent::SetTrackInput {
                            track_id: track.id,
                            input: Some(option.clone()),
                        });
                        ui.close();
                    }
                }
                if selected.is_some() && ui.button("Clear input").clicked() {
                    self.emit(UiIntent::SetTrackInput {
                        track_id: track.id,
                        input: None,
                    });
                    ui.close();
                }
            });
        if track.record_armed {
            ui.label(
                RichText::new("Monitoring while armed")
                    .small()
                    .color(theme::ACCENT),
            );
        }
    }

    fn track_monitor_editor(&mut self, ui: &mut egui::Ui, track: &Track) {
        egui::ComboBox::from_id_salt(("track-monitor", track.id))
            .selected_text(track.io.monitor.label())
            .width(ui.available_width().min(280.0))
            .show_ui(ui, |ui| {
                for mode in [
                    notwig_core::MonitorMode::Disabled,
                    notwig_core::MonitorMode::Enabled,
                    notwig_core::MonitorMode::LowLatency,
                ] {
                    if ui
                        .selectable_label(track.io.monitor == mode, mode.label())
                        .clicked()
                    {
                        self.emit(UiIntent::SetTrackMonitorMode {
                            track_id: track.id,
                            mode,
                        });
                        ui.close();
                    }
                }
            });
        if track.io.monitor == notwig_core::MonitorMode::LowLatency {
            ui.label(
                RichText::new("Runs through the track chain but bypasses groups, sends, and master processing.")
                    .small()
                    .color(theme::TEXT_MUTED),
            );
        }
    }

    fn track_comment_editor(&mut self, ui: &mut egui::Ui, track: &Track) {
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

    pub(super) fn track_info(
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
                theme::heading(ui.ctx(), track_kind_title(track.kind)).color(if track.active {
                    theme::HEADING
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
        if track.kind == TrackKind::Audio {
            ui.separator();
            ui.label(theme::heading(ui.ctx(), "INPUT"));
            self.track_input_editor(ui, &track, runtime);
            ui.separator();
            ui.label(theme::heading(ui.ctx(), "MONITORING"));
            self.track_monitor_editor(ui, &track);
        }
        ui.separator();

        let devices = doc
            .chains
            .get(&track.main_chain)
            .map(|chain| chain.devices.clone())
            .unwrap_or_default();
        let latency = chain_latency(doc, &self.registry, track.main_chain);
        let has_devices = !devices.is_empty();
        let device_height = (ui.available_height() - 310.0).max(80.0);
        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), device_height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.label(theme::heading(ui.ctx(), "DEVICES"));
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
                        if has_devices {
                            ui.add_space(2.0);
                            ui.label(
                                RichText::new(format!(
                                    "Chain latency {}",
                                    latency_label(latency, doc.meta.sample_rate.value())
                                ))
                                .small()
                                .monospace()
                                .color(theme::TEXT_MUTED),
                            );
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
                        if info_record_button(ui, track.record_armed) {
                            self.emit(UiIntent::SetTrackRecordArm {
                                track_id,
                                armed: !track.record_armed,
                            });
                        }
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
                            pan: Some(Pan(pan)),
                            mute: None,
                            solo: None,
                        });
                    }
                    ui.add_space(4.0);
                    let peak = runtime
                        .meters
                        .get(&track_id)
                        .map_or(0.0, |meter| meter.peak_l.max(meter.peak_r));
                    let held_peak = self.state.record_peak(track_id, peak);
                    if peak_label(ui, track_id, held_peak) {
                        self.state.reset_peak(track_id);
                    }
                    if let Some(volume) =
                        volume_meter_fader(ui, runtime, track_id, track.mixer.volume_db.0)
                    {
                        self.emit(UiIntent::SetTrackMixer {
                            track_id,
                            volume_db: Some(Db(volume)),
                            pan: None,
                            mute: None,
                            solo: None,
                        });
                    }
                },
            );
        });
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

fn track_kind_title(kind: TrackKind) -> &'static str {
    match kind {
        TrackKind::Audio => "AUDIO TRACK",
        TrackKind::Instrument => "INSTRUMENT TRACK",
        TrackKind::Group => "GROUP TRACK",
        TrackKind::BusFx => "FX TRACK",
        TrackKind::Master => "MASTER TRACK",
    }
}

fn info_record_button(ui: &mut egui::Ui, armed: bool) -> bool {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(34.0, 24.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        egui::CornerRadius::same(3),
        if armed {
            Color32::from_rgb(150, 48, 48)
        } else {
            theme::GRID
        },
    );
    crate::icons::paint(
        painter,
        rect,
        crate::icons::Icon::Record,
        Color32::from_rgb(231, 91, 82),
    );
    ui.interact(rect, egui::Id::new("sidebar-record"), Sense::click())
        .on_hover_text(if armed {
            "Disarm recording"
        } else {
            "Arm for recording"
        })
        .clicked()
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

fn peak_label(ui: &mut egui::Ui, track_id: TrackId, held_peak: f32) -> bool {
    let peak = held_peak.max(1e-6);
    let db = 20.0 * peak.log10();
    let color = if db >= 0.0 {
        Color32::from_rgb(224, 80, 66)
    } else {
        theme::TEXT_MUTED
    };
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 14.0), Sense::hover());
    let response = ui.interact(
        rect,
        egui::Id::new(("sidebar-peak", track_id)),
        Sense::click(),
    );
    // Keep the readout above the meter bars, not above the volume fader.
    ui.painter().text(
        egui::pos2(rect.left() + 50.0, rect.center().y),
        Align2::CENTER_CENTER,
        format!("{db:+.1}"),
        FontId::monospace(10.0),
        color,
    );
    response
        .on_hover_text("Peak since reset (click to reset)")
        .clicked()
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
        format_db(Db(volume_db)),
        FontId::monospace(10.0),
        theme::ACCENT,
    );
    ((value - volume_db).abs() > 1.0e-4).then_some(value)
}
