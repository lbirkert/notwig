use egui::{Color32, Rect, RichText};
use notwig_core::ProjectDoc;

use crate::icons::{Icon, icon_button_sized};
use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::theme;

use super::NotwigUi;

const TOP_BAR_HEIGHT: f32 = 34.0;
const TOP_BAR_ICON: f32 = 30.0;
const TOP_BAR_FONT: f32 = 12.0;
const TOP_BAR_READOUT_FONT: f32 = 11.0;

impl NotwigUi {
    pub(super) fn top_bar(&mut self, ctx: &egui::Context, doc: &ProjectDoc, runtime: &RuntimeView) {
        egui::TopBottomPanel::top("notwig_top")
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL_LIGHT)
                    .inner_margin(egui::Margin::symmetric(8, 2)),
            )
            .show(ctx, |ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), TOP_BAR_HEIGHT),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        self.file_menu(ui, doc, runtime);
                        ui.add_space(8.0);
                        ui.scope(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            let play_icon = if runtime.playing {
                                Icon::Pause
                            } else {
                                Icon::Play
                            };
                            if icon_button_sized(
                                ui,
                                play_icon,
                                egui::vec2(TOP_BAR_ICON, TOP_BAR_ICON),
                                runtime.playing,
                                if runtime.playing { "Pause" } else { "Play" },
                            )
                            .clicked()
                            {
                                if runtime.playing {
                                    self.emit(UiIntent::Pause);
                                } else {
                                    self.emit(UiIntent::Play);
                                }
                            }
                            if icon_button_sized(
                                ui,
                                Icon::Stop,
                                egui::vec2(TOP_BAR_ICON, TOP_BAR_ICON),
                                false,
                                "Stop",
                            )
                            .clicked()
                            {
                                self.emit(UiIntent::Stop);
                            }
                            if record_button(ui, doc.transport.recording) {
                                self.emit(UiIntent::ToggleRecord);
                            }
                            if icon_button_sized(
                                ui,
                                Icon::Loop,
                                egui::vec2(TOP_BAR_ICON, TOP_BAR_ICON),
                                doc.transport.loop_enabled,
                                "Toggle loop",
                            )
                            .clicked()
                            {
                                self.emit(UiIntent::ToggleLoop);
                            }
                            if icon_button_sized(
                                ui,
                                Icon::Metronome,
                                egui::vec2(TOP_BAR_ICON, TOP_BAR_ICON),
                                doc.transport.metronome_enabled,
                                "Toggle metronome",
                            )
                            .clicked()
                            {
                                self.emit(UiIntent::ToggleMetronome);
                            }
                        });
                        let engine_width = 400.0;
                        let total_width = ui.max_rect().width();
                        let left_used = ui.cursor().left() - ui.max_rect().left();
                        ui.add_space(((total_width - engine_width) * 0.5 - left_used).max(0.0));
                        ui.allocate_ui_with_layout(
                            egui::vec2(engine_width, TOP_BAR_HEIGHT),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                let engine = egui::Frame::new()
                                    .fill(Color32::from_rgb(16, 17, 21))
                                    .corner_radius(egui::CornerRadius::same(4))
                                    .inner_margin(egui::Margin::symmetric(8, 2))
                                    .show(ui, |ui| {
                                        ui.set_min_size(egui::vec2(
                                            engine_width - 16.0,
                                            TOP_BAR_HEIGHT - 8.0,
                                        ));
                                        ui.horizontal(|ui| {
                                            if cpu_component(
                                                ui,
                                                runtime.cpu_load.unwrap_or_default(),
                                            )
                                            .clicked()
                                            {
                                                self.dsp_open = true;
                                            }
                                            ui.separator();
                                            let mut tempo = doc.meta.tempo_bpm;
                                            ui.scope(|ui| {
                                                ui.visuals_mut().override_text_color =
                                                    Some(Color32::from_rgb(104, 164, 236));
                                                ui.style_mut().text_styles.insert(
                                                    egui::TextStyle::Button,
                                                    egui::FontId::monospace(TOP_BAR_FONT),
                                                );
                                                if ui
                                                    .add(
                                                        egui::DragValue::new(&mut tempo)
                                                            .range(20.0..=400.0)
                                                            .speed(0.1)
                                                            .fixed_decimals(2),
                                                    )
                                                    .on_hover_text("Tempo (BPM)")
                                                    .changed()
                                                {
                                                    self.emit(UiIntent::SetTempo(tempo));
                                                }
                                            });
                                            ui.separator();
                                            let mut numerator = doc.meta.time_sig_num;
                                            ui.scope(|ui| {
                                                ui.visuals_mut().override_text_color =
                                                    Some(Color32::from_rgb(104, 164, 236));
                                                ui.style_mut().text_styles.insert(
                                                    egui::TextStyle::Button,
                                                    egui::FontId::monospace(TOP_BAR_FONT),
                                                );
                                                if ui
                                                    .add(
                                                        egui::DragValue::new(&mut numerator)
                                                            .range(1..=32),
                                                    )
                                                    .changed()
                                                {
                                                    self.emit(UiIntent::SetTimeSignature {
                                                        numerator,
                                                        denominator: doc.meta.time_sig_den,
                                                    });
                                                }
                                            });
                                            ui.label(
                                                RichText::new("/")
                                                    .size(TOP_BAR_FONT)
                                                    .monospace()
                                                    .color(theme::TEXT_MUTED),
                                            );
                                            let mut denominator = doc.meta.time_sig_den;
                                            egui::ComboBox::from_id_salt("time-signature-den")
                                                .selected_text(
                                                    RichText::new(denominator.to_string())
                                                        .monospace()
                                                        .size(TOP_BAR_FONT)
                                                        .color(Color32::from_rgb(104, 164, 236)),
                                                )
                                                .width(32.0)
                                                .show_ui(ui, |ui| {
                                                    for value in [1u8, 2, 4, 8, 16, 32] {
                                                        if ui
                                                            .selectable_value(
                                                                &mut denominator,
                                                                value,
                                                                value.to_string(),
                                                            )
                                                            .clicked()
                                                        {
                                                            self.emit(UiIntent::SetTimeSignature {
                                                                numerator: doc.meta.time_sig_num,
                                                                denominator: value,
                                                            });
                                                        }
                                                    }
                                                });
                                            ui.separator();
                                            ui.scope(|ui| {
                                                ui.spacing_mut().item_spacing.y = 0.0;
                                                ui.vertical(|ui| {
                                                    let blue = Color32::from_rgb(104, 164, 236);
                                                    ui.label(
                                                        RichText::new(format_position_bars(
                                                            runtime.playhead,
                                                            doc.meta.bar_length(),
                                                            doc.meta.beat_length(),
                                                        ))
                                                        .monospace()
                                                        .size(TOP_BAR_READOUT_FONT)
                                                        .color(blue),
                                                    );
                                                    ui.label(
                                                        RichText::new(format_position_time(
                                                            runtime.playhead,
                                                            doc.meta.tempo_bpm,
                                                        ))
                                                        .monospace()
                                                        .size(TOP_BAR_READOUT_FONT)
                                                        .color(blue),
                                                    );
                                                });
                                            });
                                            ui.separator();
                                            let master = doc
                                                .master_track()
                                                .and_then(|id| runtime.meters.get(&id));
                                            master_meter(ui, master);
                                        });
                                    });
                                engine.response.context_menu(|ui| {
                                    if ui.button("Stop playback").clicked() {
                                        self.emit(UiIntent::Stop);
                                        ui.close();
                                    }
                                    if runtime.engine_stopped {
                                        if ui.button("Start audio engine").clicked() {
                                            self.emit(UiIntent::RestartAudioEngine);
                                            ui.close();
                                        }
                                    } else {
                                        if ui.button("Stop audio engine").clicked() {
                                            self.emit(UiIntent::StopAudioEngine);
                                            ui.close();
                                        }
                                        if ui.button("Restart audio engine").clicked() {
                                            self.emit(UiIntent::RestartAudioEngine);
                                            ui.close();
                                        }
                                    }
                                    if ui.button("Audio settings…").clicked() {
                                        self.open_settings(runtime);
                                        ui.close();
                                    }
                                });
                            },
                        );
                    },
                );
            });
    }

    fn file_menu(&mut self, ui: &mut egui::Ui, doc: &ProjectDoc, runtime: &RuntimeView) {
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(64.0, TOP_BAR_ICON), egui::Sense::click());
        let painter = ui.painter();
        painter.rect_filled(
            rect,
            egui::CornerRadius::same(4),
            if response.hovered() {
                theme::GRID_STRONG
            } else {
                theme::PANEL
            },
        );
        painter.rect_stroke(
            rect,
            egui::CornerRadius::same(4),
            egui::Stroke::new(1.0_f32, theme::GRID),
            egui::StrokeKind::Inside,
        );
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "FILE",
            egui::FontId::monospace(10.0),
            Color32::WHITE,
        );
        egui::Popup::menu(&response).show(|ui| {
            ui.menu_button("Recent Projects", |ui| {
                if runtime.recent_projects.is_empty() {
                    ui.add_enabled(false, egui::Button::new("No recent projects"));
                } else {
                    for project in &runtime.recent_projects {
                        if ui.button(&project.label).clicked() {
                            self.emit(UiIntent::OpenProjectNamed(project.key.clone()));
                            ui.close();
                        }
                    }
                }
            });
            if ui.button("New Project").clicked() {
                self.emit(UiIntent::NewProject);
                ui.close();
            }
            if ui.button("Open Project").clicked() {
                self.emit(UiIntent::OpenProject);
                ui.close();
            }
            if ui.button("Save Project").clicked() {
                self.emit(UiIntent::Save);
                ui.close();
            }
            if ui.button("Save As").clicked() {
                self.emit(UiIntent::SaveAs);
                ui.close();
            }
            #[cfg(target_arch = "wasm32")]
            if ui.button("Download Project").clicked() {
                self.emit(UiIntent::DownloadProject);
                ui.close();
            }
            ui.separator();
            if ui.button("Import Audio…").clicked() {
                self.emit(UiIntent::ImportAudio);
                ui.close();
            }
            if ui.button("Settings…").clicked() {
                self.open_settings(runtime);
                ui.close();
            }
            if ui.button("Render Mixdown…").clicked() {
                self.open_render(doc);
                ui.close();
            }
        });
    }
}

pub(crate) fn cpu_component(ui: &mut egui::Ui, cpu: f32) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(56.0, TOP_BAR_HEIGHT - 8.0), egui::Sense::click());
    let painter = ui.painter();
    let icon = Rect::from_center_size(
        egui::pos2(rect.left() + 9.0, rect.center().y),
        egui::Vec2::splat(10.0),
    );
    let blue = Color32::from_rgb(80, 145, 230);
    painter.rect_stroke(
        icon,
        egui::CornerRadius::same(1),
        egui::Stroke::new(1.0_f32, blue),
        egui::StrokeKind::Inside,
    );
    for offset in [-3.0_f32, 0.0, 3.0] {
        painter.line_segment(
            [
                egui::pos2(icon.left() - 2.0, icon.center().y + offset),
                egui::pos2(icon.left(), icon.center().y + offset),
            ],
            egui::Stroke::new(1.0_f32, blue),
        );
        painter.line_segment(
            [
                egui::pos2(icon.right(), icon.center().y + offset),
                egui::pos2(icon.right() + 2.0, icon.center().y + offset),
            ],
            egui::Stroke::new(1.0_f32, blue),
        );
    }
    let bar = Rect::from_min_max(
        egui::pos2(rect.left() + 21.0, rect.center().y - 2.5),
        egui::pos2(rect.right() - 2.0, rect.center().y + 2.5),
    );
    painter.rect_filled(bar, egui::CornerRadius::ZERO, theme::GRID);
    let fill = Rect::from_min_max(
        bar.min,
        egui::pos2(
            bar.left() + bar.width() * (cpu / 100.0).clamp(0.0, 1.0),
            bar.bottom(),
        ),
    );
    painter.rect_filled(fill, egui::CornerRadius::ZERO, blue);
    response.on_hover_text("DSP load — click for analysis")
}

fn record_button(ui: &mut egui::Ui, recording: bool) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(TOP_BAR_ICON, TOP_BAR_ICON), egui::Sense::click());
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        egui::CornerRadius::same(4),
        if recording {
            Color32::from_rgb(86, 34, 34)
        } else if response.hovered() {
            theme::GRID_STRONG
        } else {
            theme::PANEL
        },
    );
    painter.rect_stroke(
        rect,
        egui::CornerRadius::same(4),
        egui::Stroke::new(
            1.0_f32,
            if recording {
                Color32::from_rgb(231, 91, 82)
            } else {
                theme::GRID
            },
        ),
        egui::StrokeKind::Inside,
    );
    crate::icons::paint(painter, rect, Icon::Record, Color32::from_rgb(231, 91, 82));
    response
        .on_hover_text(if recording {
            "Turn record mode off"
        } else {
            "Turn record mode on"
        })
        .clicked()
}

fn format_position_bars(
    time: notwig_core::TimeMus,
    bar_length: notwig_core::TimeMus,
    beat_length: notwig_core::TimeMus,
) -> String {
    let beats = time.as_f64().max(0.0);
    let bar_len = bar_length.as_f64().max(f64::EPSILON);
    let beat_len = beat_length.as_f64().max(f64::EPSILON);
    let bar = (beats / bar_len).floor() as u64 + 1;
    let beat_pos = beats % bar_len;
    let beat = (beat_pos / beat_len).floor() as u64 + 1;
    let sub_beat = (beat_pos % beat_len) / beat_len;
    let sixteenth = (sub_beat * 4.0).floor() as u64 + 1;
    let tick = ((sub_beat * 4.0).fract() * 100.0).floor() as u64;
    format!("{bar}.{beat}.{sixteenth}.{tick:02}")
}

fn format_position_time(time: notwig_core::TimeMus, tempo_bpm: f64) -> String {
    let seconds = (time.as_f64() * 60.0 / tempo_bpm.max(1.0)).max(0.0);
    let minutes = (seconds / 60.0).floor() as u64;
    let seconds = seconds % 60.0;
    format!("{minutes}:{seconds:06.3}")
}

fn master_meter(ui: &mut egui::Ui, meter: Option<&crate::runtime::MeterValues>) {
    let meter = meter.copied().unwrap_or_default();
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(14.0, TOP_BAR_HEIGHT - 10.0),
        egui::Sense::hover(),
    );
    crate::widgets::draw_stereo_meter(ui.painter(), rect, meter.peak_l, meter.peak_r);
}
