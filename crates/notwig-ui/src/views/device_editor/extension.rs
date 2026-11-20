use std::collections::BTreeMap;

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use notwig_core::{DeviceInstance, Norm01, ParamValue};
use notwig_dsp::DeviceSpec;
use notwig_ext_api::{
    Color, DeviceDrawContext, DrawCommand, PanelCommand, PanelDraw, Point, TextAlign, ThemeSnapshot,
};

use crate::shell::NotwigUi;
use crate::theme;

impl NotwigUi {
    pub(crate) fn draw_extension_device_panel(
        &mut self,
        ui: &mut egui::Ui,
        device: &DeviceInstance,
        spec: &DeviceSpec,
        sample_rate: u32,
        size: Vec2,
    ) -> bool {
        let selected = self.state.selection.devices.contains(&device.id);
        let context = device_context(device, spec, sample_rate, size, selected);
        let panel = {
            let Some(host) = self.device_panel_host.as_mut() else {
                return false;
            };
            host.draw_device_panel(&device.type_id, &context)
        };
        let Some(panel) = panel else {
            return false;
        };
        if panel.validate().is_err() {
            return false;
        }
        let (target, _) = ui.allocate_exact_size(size, Sense::hover());
        self.paint_panel(ui, device, spec, &panel, target);
        true
    }

    fn paint_panel(
        &mut self,
        ui: &mut egui::Ui,
        device: &DeviceInstance,
        spec: &DeviceSpec,
        panel: &PanelDraw,
        target: Rect,
    ) {
        let mut clips = vec![target];
        for command in &panel.commands {
            match command {
                PanelCommand::Draw(command) => match command {
                    DrawCommand::PushClip { rect } => {
                        let clip = to_rect(*rect, target.min);
                        let current = *clips.last().unwrap_or(&target);
                        clips.push(current.intersect(clip));
                    }
                    DrawCommand::PopClip => {
                        if clips.len() > 1 {
                            clips.pop();
                        }
                    }
                    other => {
                        let clip = *clips.last().unwrap_or(&target);
                        paint_draw(ui, other, target.min, clip);
                    }
                },
                PanelCommand::Knob {
                    rect,
                    param_id,
                    label,
                    bipolar,
                } => {
                    let Some(param) = spec.params.iter().find(|param| param.id == *param_id) else {
                        continue;
                    };
                    let rect = to_rect(*rect, target.min);
                    let value = device
                        .param_values
                        .get(param_id)
                        .cloned()
                        .unwrap_or_else(|| param.default.clone());
                    let mut norm = param.norm(&value).0;
                    let default_norm = param.norm(&param.default).0;
                    let changed = crate::widgets::knob(
                        ui,
                        ui.make_persistent_id(("extension-knob", device.id, *param_id)),
                        rect,
                        norm,
                        default_norm,
                        *bipolar,
                        &param.name,
                    );
                    if let Some(changed) = changed {
                        norm = changed;
                        self.emit(crate::intent::UiIntent::SetParam {
                            device_id: device.id,
                            param_id: *param_id,
                            value: param.value_from_norm(Norm01::clamped(norm)),
                        });
                    }
                    ui.painter().text(
                        Rect::from_min_max(
                            egui::pos2(rect.left(), rect.bottom()),
                            egui::pos2(rect.right(), rect.bottom() + 14.0),
                        )
                        .center(),
                        Align2::CENTER_TOP,
                        label,
                        FontId::proportional(10.0),
                        theme::TEXT_MUTED,
                    );
                }
                PanelCommand::Toggle {
                    rect,
                    param_id,
                    label,
                } => {
                    let rect = to_rect(*rect, target.min);
                    let value = device.param_values.get(param_id);
                    let active = matches!(value, Some(ParamValue::Bool(true)));
                    let response = ui.interact(
                        rect,
                        ui.make_persistent_id(("extension-toggle", device.id, *param_id)),
                        Sense::click(),
                    );
                    let painter = ui.painter();
                    painter.rect_filled(
                        rect,
                        egui::CornerRadius::same(4),
                        if active {
                            theme::ACCENT
                        } else {
                            theme::PANEL_LIGHT
                        },
                    );
                    painter.text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        label,
                        FontId::proportional(10.0),
                        if active {
                            Color32::BLACK
                        } else {
                            theme::TEXT_MUTED
                        },
                    );
                    if response.clicked() {
                        self.emit(crate::intent::UiIntent::SetParam {
                            device_id: device.id,
                            param_id: *param_id,
                            value: ParamValue::Bool(!active),
                        });
                    }
                }
                PanelCommand::Label { rect, text } => {
                    let rect = to_rect(*rect, target.min);
                    ui.painter().with_clip_rect(target).text(
                        rect.min,
                        Align2::LEFT_TOP,
                        text,
                        FontId::proportional(11.0),
                        theme::TEXT_MUTED,
                    );
                }
                PanelCommand::Section { rect, title } => {
                    let rect = to_rect(*rect, target.min);
                    ui.painter().with_clip_rect(target).text(
                        rect.min,
                        Align2::LEFT_TOP,
                        title.to_uppercase(),
                        FontId::monospace(9.0),
                        theme::TEXT_MUTED,
                    );
                }
            }
        }
    }
}

fn device_context(
    device: &DeviceInstance,
    spec: &DeviceSpec,
    sample_rate: u32,
    size: Vec2,
    selected: bool,
) -> DeviceDrawContext {
    DeviceDrawContext {
        device_id: device.id.to_string(),
        type_id: device.type_id.clone(),
        name: device.name.clone().unwrap_or_else(|| spec.name.clone()),
        sample_rate,
        width: size.x,
        height: size.y,
        selected,
        bypassed: device.bypass,
        params: device
            .param_values
            .iter()
            .map(|(id, value)| (*id, value.clone()))
            .collect::<BTreeMap<_, _>>(),
        theme: theme_snapshot(),
    }
}

fn theme_snapshot() -> ThemeSnapshot {
    ThemeSnapshot {
        accent: to_color32(theme::ACCENT),
        accent_dim: to_color32(theme::ACCENT_DIM),
        playhead: to_color32(theme::PLAYHEAD),
        panel: to_color32(theme::PANEL),
        panel_light: to_color32(theme::PANEL_LIGHT),
        lane: to_color32(theme::LANE),
        grid: to_color32(theme::GRID),
        grid_strong: to_color32(theme::GRID_STRONG),
        text: Color::rgb(235, 237, 242),
        text_muted: to_color32(theme::TEXT_MUTED),
        selection: to_color32(theme::SELECTION),
        clip_audio: to_color32(theme::CLIP_AUDIO),
        clip_midi: to_color32(theme::CLIP_MIDI),
    }
}

fn paint_draw(ui: &egui::Ui, command: &DrawCommand, origin: Pos2, clip: Rect) {
    let painter = ui.painter().with_clip_rect(clip);
    match command {
        DrawCommand::PushClip { .. } | DrawCommand::PopClip => {}
        DrawCommand::FillRect {
            rect,
            radius,
            color,
        } => {
            painter.rect_filled(
                to_rect(*rect, origin),
                egui::CornerRadius::same(radius.max(0.0) as u8),
                to_color(*color),
            );
        }
        DrawCommand::StrokeRect {
            rect,
            radius,
            width,
            color,
        } => {
            painter.rect_stroke(
                to_rect(*rect, origin),
                egui::CornerRadius::same(radius.max(0.0) as u8),
                Stroke::new(*width, to_color(*color)),
                StrokeKind::Inside,
            );
        }
        DrawCommand::Line {
            from,
            to,
            width,
            color,
        } => {
            painter.line_segment(
                [to_pos(*from, origin), to_pos(*to, origin)],
                Stroke::new(*width, to_color(*color)),
            );
        }
        DrawCommand::CircleFill {
            center,
            radius,
            color,
        } => {
            painter.circle_filled(to_pos(*center, origin), *radius, to_color(*color));
        }
        DrawCommand::CircleStroke {
            center,
            radius,
            width,
            color,
        } => {
            painter.circle_stroke(
                to_pos(*center, origin),
                *radius,
                Stroke::new(*width, to_color(*color)),
            );
        }
        DrawCommand::Text {
            pos,
            text,
            size,
            color,
            align,
        } => {
            painter.text(
                to_pos(*pos, origin),
                match align {
                    TextAlign::Left => Align2::LEFT_TOP,
                    TextAlign::Center => Align2::CENTER_TOP,
                    TextAlign::Right => Align2::RIGHT_TOP,
                },
                text,
                FontId::monospace(*size),
                to_color(*color),
            );
        }
        DrawCommand::Path {
            points,
            closed,
            fill,
            stroke,
        } => {
            let points: Vec<Pos2> = points.iter().map(|point| to_pos(*point, origin)).collect();
            if *closed {
                if let Some(fill) = fill {
                    if points.len() >= 3 {
                        painter.add(egui::Shape::convex_polygon(
                            points.clone(),
                            to_color(*fill),
                            Stroke::NONE,
                        ));
                    }
                }
            }
            if let Some(stroke) = stroke {
                painter.add(egui::Shape::line(
                    points,
                    Stroke::new(stroke.width, to_color(stroke.color)),
                ));
            }
        }
    }
}

fn to_pos(point: Point, origin: Pos2) -> Pos2 {
    egui::pos2(origin.x + point.x, origin.y + point.y)
}

fn to_rect(rect: notwig_ext_api::Rect, origin: Pos2) -> Rect {
    Rect::from_min_max(to_pos(rect.min, origin), to_pos(rect.max, origin))
}

fn to_color(color: Color) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a)
}

fn to_color32(color: Color32) -> Color {
    let [r, g, b, a] = color.to_array();
    Color::rgba(r, g, b, a)
}
