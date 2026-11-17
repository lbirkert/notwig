use std::collections::BTreeMap;

use egui::{Align, Align2, Color32, CornerRadius, FontId, Rect, RichText, Sense, Stroke, Vec2};
use notwig_core::{DeviceId, DeviceInstance, ParamId, ParamValue};

use crate::intent::UiIntent;
use crate::shell::NotwigUi;
use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EqDragMode {
    GainFrequency,
    Q,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EqDrag {
    device_id: DeviceId,
    band: usize,
    mode: EqDragMode,
    start_freq: f64,
    start_gain: f64,
    start_q: f64,
    start_pointer: egui::Pos2,
}

impl NotwigUi {
    fn eq_curve(
        &mut self,
        ui: &mut egui::Ui,
        device: &DeviceInstance,
        sample_rate: f64,
        height: f32,
    ) {
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width().max(220.0), height),
            Sense::click_and_drag(),
        );
        let painter = ui.painter().clone();
        painter.rect_filled(rect, CornerRadius::same(4), Color32::from_rgb(16, 18, 22));
        let min_f = 20.0_f64;
        let max_f = 20_000.0_f64;
        let log_min = min_f.log10();
        let log_max = max_f.log10();
        let range = self.eq_range.max(10.0);
        let x_for_freq = |freq: f64| {
            rect.left()
                + ((freq.max(min_f).min(max_f).log10() - log_min) / (log_max - log_min)) as f32
                    * rect.width()
        };
        let y_for_db = |db: f64| {
            rect.center().y
                - (db.clamp(-f64::from(range), f64::from(range)) / f64::from(range)) as f32
                    * rect.height()
                    * 0.44
        };
        let freq_for_x = |x: f32| {
            let t = ((x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0);
            min_f * (max_f / min_f).powf(f64::from(t))
        };
        let db_for_y = |y: f32| {
            f64::from(((rect.center().y - y) / (rect.height() * 0.44)).clamp(-1.0, 1.0))
                * f64::from(range)
        };
        for (freq, label) in [
            (20.0, "20"),
            (50.0, ""),
            (100.0, "100"),
            (200.0, ""),
            (500.0, ""),
            (1_000.0, "1k"),
            (2_000.0, ""),
            (5_000.0, ""),
            (10_000.0, "10k"),
            (20_000.0, ""),
        ] {
            let x = x_for_freq(freq);
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                Stroke::new(1.0_f32, theme::GRID),
            );
            if !label.is_empty() {
                painter.text(
                    egui::pos2(x + 3.0, rect.bottom() - 2.0),
                    Align2::LEFT_BOTTOM,
                    label,
                    FontId::monospace(8.0),
                    theme::TEXT_MUTED,
                );
            }
        }
        for db in [-30.0_f64, -20.0, -10.0, 0.0, 10.0, 20.0, 30.0] {
            if db.abs() > f64::from(range) {
                continue;
            }
            let y = y_for_db(db);
            painter.line_segment(
                [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                Stroke::new(
                    1.0_f32,
                    if db == 0.0 {
                        theme::GRID_STRONG
                    } else {
                        theme::GRID
                    },
                ),
            );
            if db == f64::from(range) || db == -f64::from(range) {
                painter.text(
                    egui::pos2(rect.right() - 3.0, y + 1.0),
                    Align2::RIGHT_TOP,
                    format!("{db:+.0}"),
                    FontId::monospace(8.0),
                    theme::TEXT_MUTED,
                );
            }
        }
        let values = self.device_values(device);
        let mut points: Vec<Option<egui::Pos2>> = Vec::with_capacity(241);
        for index in 0..=240u32 {
            let t = f64::from(index) / 240.0;
            let freq = 10.0_f64.powf(log_min + t * (log_max - log_min));
            let magnitude = notwig_dsp::stock::eq_magnitude_response(&values, sample_rate, freq);
            let db = 20.0 * magnitude.max(1e-12).log10();
            let point = (db.is_finite() && db >= -f64::from(range) && db <= f64::from(range))
                .then(|| egui::pos2(x_for_freq(freq), y_for_db(db)));
            points.push(point);
        }
        for pair in points.windows(2) {
            let (Some(start), Some(end)) = (pair[0], pair[1]) else {
                continue;
            };
            let t = (start.x - rect.left()) / rect.width();
            painter.line_segment([start, end], Stroke::new(2.2_f32, curve_color(t)));
        }
        let band = self.eq_band.min(3);
        let selected_freq = values
            .get(&notwig_dsp::stock::band_param(band, 2))
            .and_then(ParamValue::as_f64)
            .unwrap_or(1_000.0);
        let selected_enabled = matches!(
            values.get(&notwig_dsp::stock::band_param(band, 0)),
            Some(ParamValue::Bool(true))
        );
        if selected_enabled {
            let line_x = x_for_freq(selected_freq);
            painter.line_segment(
                [
                    egui::pos2(line_x, rect.top()),
                    egui::pos2(line_x, rect.bottom()),
                ],
                Stroke::new(1.0_f32, Color32::from_white_alpha(210)),
            );
        }
        let mut point_positions = Vec::new();
        for band_index in 0..4 {
            let enabled = matches!(
                values.get(&notwig_dsp::stock::band_param(band_index, 0)),
                Some(ParamValue::Bool(true))
            );
            let freq = values
                .get(&notwig_dsp::stock::band_param(band_index, 2))
                .and_then(ParamValue::as_f64)
                .unwrap_or(1_000.0);
            let gain = values
                .get(&notwig_dsp::stock::band_param(band_index, 3))
                .and_then(ParamValue::as_f64)
                .unwrap_or(0.0);
            point_positions.push((
                band_index,
                egui::pos2(x_for_freq(freq), y_for_db(gain)),
                enabled,
            ));
        }
        let hover_pos = response.hover_pos();
        let hovered_band = hover_pos.and_then(|pos| {
            point_positions
                .iter()
                .find(|(_, point, enabled)| *enabled && pos.distance(*point) <= 14.0)
                .map(|(band_index, _, _)| *band_index)
        });
        let mut handles = Vec::new();
        for (band_index, pos, enabled) in &point_positions {
            if !enabled {
                continue;
            }
            let active = *band_index == band;
            let show_handle = active || Some(*band_index) == hovered_band;
            let fill = if active {
                Color32::WHITE
            } else {
                Color32::from_gray(220)
            };
            painter.circle_filled(*pos, 9.0, fill);
            painter.circle_stroke(
                *pos,
                9.0,
                Stroke::new(
                    if active { 2.0_f32 } else { 1.0_f32 },
                    if active {
                        theme::ACCENT
                    } else {
                        Color32::from_black_alpha(160)
                    },
                ),
            );
            painter.text(
                *pos,
                Align2::CENTER_CENTER,
                format!("{}", band_index + 1),
                FontId::proportional(10.0),
                Color32::from_black_alpha(220),
            );
            if show_handle {
                let handle = Rect::from_center_size(
                    egui::pos2(pos.x, (pos.y - 22.0).max(rect.top() + 8.0)),
                    Vec2::new(22.0, 13.0),
                );
                painter.rect_filled(
                    handle,
                    CornerRadius::same(6),
                    if active {
                        Color32::WHITE
                    } else {
                        Color32::from_gray(225)
                    },
                );
                painter.text(
                    handle.center(),
                    Align2::CENTER_CENTER,
                    "↔",
                    FontId::proportional(10.0),
                    Color32::from_black_alpha(190),
                );
                handles.push((*band_index, handle));
            }
        }
        if response.drag_started() {
            if let Some(pos) = ui
                .input(|input| input.pointer.press_origin())
                .or_else(|| response.interact_pointer_pos())
            {
                let mut started: Option<(usize, EqDragMode, bool)> = None;
                if let Some((band_index, _)) =
                    handles.iter().find(|(_, handle)| handle.contains(pos))
                {
                    started = Some((*band_index, EqDragMode::Q, false));
                } else if let Some((band_index, _, _)) = point_positions
                    .iter()
                    .find(|(_, point, enabled)| *enabled && pos.distance(*point) <= 13.0)
                {
                    started = Some((*band_index, EqDragMode::GainFrequency, false));
                } else if let Some((band_index, _, _)) =
                    point_positions.iter().find(|(_, _, enabled)| !*enabled)
                {
                    started = Some((*band_index, EqDragMode::GainFrequency, true));
                }
                if let Some((band_index, mode, enable)) = started {
                    self.eq_band = band_index;
                    if enable {
                        self.emit(UiIntent::SetParam {
                            device_id: device.id,
                            param_id: notwig_dsp::stock::band_param(band_index, 0),
                            value: ParamValue::Bool(true),
                        });
                    }
                    let (freq, gain, q) = if enable {
                        let freq = freq_for_x(pos.x);
                        let gain = db_for_y(pos.y);
                        self.emit_eq_param(device.id, band_index, 2, freq);
                        self.emit_eq_param(device.id, band_index, 3, gain);
                        (freq, gain, 0.707)
                    } else {
                        (
                            values
                                .get(&notwig_dsp::stock::band_param(band_index, 2))
                                .and_then(ParamValue::as_f64)
                                .unwrap_or(1_000.0),
                            values
                                .get(&notwig_dsp::stock::band_param(band_index, 3))
                                .and_then(ParamValue::as_f64)
                                .unwrap_or(0.0),
                            values
                                .get(&notwig_dsp::stock::band_param(band_index, 4))
                                .and_then(ParamValue::as_f64)
                                .unwrap_or(0.707),
                        )
                    };
                    self.eq_drag = Some(EqDrag {
                        device_id: device.id,
                        band: band_index,
                        mode,
                        start_freq: freq,
                        start_gain: gain,
                        start_q: q,
                        start_pointer: pos,
                    });
                }
            }
        }
        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let enabled_hit = point_positions
                    .iter()
                    .find(|(_, point, enabled)| *enabled && pos.distance(*point) <= 13.0);
                if let Some((band_index, _, _)) = enabled_hit {
                    self.eq_band = *band_index;
                } else if let Some((band_index, _, _)) =
                    point_positions.iter().find(|(_, _, enabled)| !*enabled)
                {
                    self.eq_band = *band_index;
                    self.emit(UiIntent::SetParam {
                        device_id: device.id,
                        param_id: notwig_dsp::stock::band_param(*band_index, 0),
                        value: ParamValue::Bool(true),
                    });
                    self.emit_eq_param(device.id, *band_index, 2, freq_for_x(pos.x));
                    self.emit_eq_param(device.id, *band_index, 3, db_for_y(pos.y));
                }
            }
        }
        if response.double_clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                if let Some((band_index, _, _)) = point_positions
                    .iter()
                    .find(|(_, point, enabled)| *enabled && pos.distance(*point) <= 13.0)
                {
                    self.eq_band = *band_index;
                    self.emit(UiIntent::SetParam {
                        device_id: device.id,
                        param_id: notwig_dsp::stock::band_param(*band_index, 0),
                        value: ParamValue::Bool(false),
                    });
                }
            }
        }
        if response.dragged() {
            if let Some(drag) = self.eq_drag {
                let Some(current) = response.interact_pointer_pos() else {
                    return;
                };
                let delta = current - drag.start_pointer;
                let width = rect.width().max(1.0);
                let frequency_factor =
                    (f64::from(delta.x) / f64::from(width)) * (max_f / min_f).ln();
                let freq = (drag.start_freq * frequency_factor.exp()).clamp(min_f, max_f);
                let gain = (drag.start_gain
                    - f64::from(delta.y) / f64::from(rect.height()) * 2.0 * f64::from(range))
                .clamp(-24.0, 24.0);
                match drag.mode {
                    EqDragMode::GainFrequency => {
                        self.emit_eq_param(drag.device_id, drag.band, 2, freq);
                        self.emit_eq_param(drag.device_id, drag.band, 3, gain);
                    }
                    EqDragMode::Q => {
                        let q =
                            (drag.start_q * (-f64::from(delta.y) * 0.02).exp()).clamp(0.1, 18.0);
                        self.emit_eq_param(drag.device_id, drag.band, 2, freq);
                        self.emit_eq_param(drag.device_id, drag.band, 4, q);
                    }
                }
            }
        }
        if response.drag_stopped() {
            self.eq_drag = None;
        }
    }
    pub(super) fn eq_card_body(
        &mut self,
        ui: &mut egui::Ui,
        device: &DeviceInstance,
        sample_rate: f64,
    ) {
        self.eq_body(ui, device, sample_rate);
    }

    fn eq_body(&mut self, ui: &mut egui::Ui, device: &DeviceInstance, sample_rate: f64) {
        let band = self.eq_band.min(3);
        let shift_id = notwig_dsp::stock::p_eq_shift();
        let gain_id = notwig_dsp::stock::p_eq_output_gain();
        ui.spacing_mut().item_spacing.y = 0.0;
        let curve_height = (ui.available_height() - 56.0).max(60.0);
        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), 24.0),
            egui::Layout::left_to_right(Align::Center),
            |ui| {
                let mut shift = device
                    .param_values
                    .get(&shift_id)
                    .and_then(ParamValue::as_f64)
                    .unwrap_or(0.0);
                ui.label(RichText::new("Shift").small().color(theme::TEXT_MUTED));
                if ui
                    .add(
                        egui::DragValue::new(&mut shift)
                            .speed(0.01)
                            .fixed_decimals(2)
                            .suffix(" st"),
                    )
                    .changed()
                {
                    self.emit(UiIntent::SetParam {
                        device_id: device.id,
                        param_id: shift_id,
                        value: ParamValue::F64(shift.clamp(-12.0, 12.0)),
                    });
                }
                ui.add_space(6.0);
                let mut gain = device
                    .param_values
                    .get(&gain_id)
                    .and_then(ParamValue::as_f64)
                    .unwrap_or(0.0);
                ui.label(RichText::new("Gain").small().color(theme::TEXT_MUTED));
                if ui
                    .add(
                        egui::DragValue::new(&mut gain)
                            .speed(0.1)
                            .fixed_decimals(1)
                            .suffix(" dB"),
                    )
                    .changed()
                {
                    self.emit(UiIntent::SetParam {
                        device_id: device.id,
                        param_id: gain_id,
                        value: ParamValue::F64(gain.clamp(-24.0, 24.0)),
                    });
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for range in [30.0_f32, 20.0, 10.0] {
                        if ui
                            .selectable_label(
                                (self.eq_range - range).abs() < 0.1,
                                format!("±{range:.0}"),
                            )
                            .clicked()
                        {
                            self.eq_range = range;
                        }
                    }
                    ui.label(RichText::new("Range").small().color(theme::TEXT_MUTED));
                });
            },
        );
        self.eq_curve(ui, device, sample_rate, curve_height);
        self.eq_band_controls(ui, device, band);
    }

    pub(crate) fn device_values(&self, device: &DeviceInstance) -> BTreeMap<ParamId, ParamValue> {
        let mut values = BTreeMap::new();
        if let Some(spec) = self.registry.resolve(&device.type_id, device.version) {
            for param in &spec.params {
                values.insert(param.id, param.default.clone());
            }
        }
        for (id, value) in &device.param_values {
            values.insert(*id, value.clone());
        }
        values
    }

    fn eq_band_controls(&mut self, ui: &mut egui::Ui, device: &DeviceInstance, band: usize) {
        let values = self.device_values(device);
        let enabled_id = notwig_dsp::stock::band_param(band, 0);
        let type_id = notwig_dsp::stock::band_param(band, 1);
        let freq_id = notwig_dsp::stock::band_param(band, 2);
        let gain_id = notwig_dsp::stock::band_param(band, 3);
        let q_id = notwig_dsp::stock::band_param(band, 4);
        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), 24.0),
            egui::Layout::left_to_right(Align::Center),
            |ui| {
                egui::ComboBox::from_id_salt(("eq_band", device.id))
                    .selected_text(format!("{}", band + 1))
                    .width(42.0)
                    .show_ui(ui, |ui| {
                        for index in 0..4usize {
                            if ui
                                .selectable_label(self.eq_band == index, format!("{}", index + 1))
                                .clicked()
                            {
                                self.eq_band = index;
                                ui.close();
                            }
                        }
                    });
                let enabled = !matches!(values.get(&enabled_id), Some(ParamValue::Bool(false)));
                if crate::icons::icon_button_sized(
                    ui,
                    crate::icons::Icon::Power,
                    Vec2::new(24.0, 20.0),
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
                        param_id: enabled_id,
                        value: ParamValue::Bool(!enabled),
                    });
                }
                let selected_type = values
                    .get(&type_id)
                    .and_then(ParamValue::as_f64)
                    .unwrap_or(0.0) as u32;
                egui::ComboBox::from_id_salt(("eq_type", device.id))
                    .selected_text(eq_type_label(selected_type))
                    .width(92.0)
                    .show_ui(ui, |ui| {
                        for index in 0..5u32 {
                            if ui
                                .selectable_label(selected_type == index, eq_type_label(index))
                                .clicked()
                            {
                                self.emit(UiIntent::SetParam {
                                    device_id: device.id,
                                    param_id: type_id,
                                    value: ParamValue::EnumIndex(index),
                                });
                                ui.close();
                            }
                        }
                    });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut q = values
                        .get(&q_id)
                        .and_then(ParamValue::as_f64)
                        .unwrap_or(0.707);
                    if ui
                        .add(egui::DragValue::new(&mut q).speed(0.01).fixed_decimals(2))
                        .changed()
                    {
                        self.emit_eq_param(device.id, band, 4, q.clamp(0.1, 18.0));
                    }
                    let mut gain = values
                        .get(&gain_id)
                        .and_then(ParamValue::as_f64)
                        .unwrap_or(0.0);
                    if ui
                        .add(
                            egui::DragValue::new(&mut gain)
                                .speed(0.1)
                                .fixed_decimals(1)
                                .suffix(" dB"),
                        )
                        .changed()
                    {
                        self.emit_eq_param(device.id, band, 3, gain.clamp(-24.0, 24.0));
                    }
                    let mut freq = values
                        .get(&freq_id)
                        .and_then(ParamValue::as_f64)
                        .unwrap_or(1_000.0);
                    if ui
                        .add(
                            egui::DragValue::new(&mut freq)
                                .speed(1.0)
                                .fixed_decimals(0)
                                .suffix(" Hz"),
                        )
                        .changed()
                    {
                        self.emit_eq_param(device.id, band, 2, freq.clamp(20.0, 20_000.0));
                    }
                });
            },
        );
    }

    fn emit_eq_param(&mut self, device_id: DeviceId, band: usize, offset: u128, value: f64) {
        self.emit(UiIntent::SetParam {
            device_id,
            param_id: notwig_dsp::stock::band_param(band, offset),
            value: ParamValue::F64(value),
        });
    }
}

fn curve_color(t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let red = (232, 72, 72);
    let green = (96, 210, 120);
    let blue = (96, 158, 240);
    let blend = |a: (u8, u8, u8), b: (u8, u8, u8), f: f32| {
        Color32::from_rgb(
            (f32::from(a.0) + (f32::from(b.0) - f32::from(a.0)) * f) as u8,
            (f32::from(a.1) + (f32::from(b.1) - f32::from(a.1)) * f) as u8,
            (f32::from(a.2) + (f32::from(b.2) - f32::from(a.2)) * f) as u8,
        )
    };
    if t < 0.5 {
        blend(red, green, t * 2.0)
    } else {
        blend(green, blue, (t - 0.5) * 2.0)
    }
}

fn eq_type_label(index: u32) -> &'static str {
    match index {
        1 => "Low Shelf",
        2 => "High Shelf",
        3 => "High Pass",
        4 => "Low Pass",
        _ => "Bell",
    }
}
