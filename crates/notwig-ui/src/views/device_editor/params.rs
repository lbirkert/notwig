use egui::{Align2, Color32, CornerRadius, FontId, Rect, RichText, Sense, Stroke, Vec2};
use notwig_core::dsp::{ParamKind, ParamSpec};
use notwig_core::{DeviceInstance, Norm01, ParamValue};

use crate::intent::UiIntent;
use crate::shell::NotwigUi;
use crate::theme;
use crate::widgets::format_param;

const PARAM_ROW_H: f32 = 54.0;

impl NotwigUi {
    pub(super) fn param_editor(&mut self, ui: &mut egui::Ui, device: &DeviceInstance) {
        let Some(spec) = self
            .registry
            .resolve(&device.type_id, device.version)
            .cloned()
        else {
            ui.label(
                RichText::new("No parameter metadata. Relink or install the plugin.")
                    .small()
                    .color(theme::TEXT_MUTED),
            );
            return;
        };
        self.plugin_param_editor(ui, device, &spec);
    }

    fn plugin_param_editor(
        &mut self,
        ui: &mut egui::Ui,
        device: &DeviceInstance,
        spec: &notwig_core::dsp::DeviceSpec,
    ) {
        let scope = ui.id().with(("plugin-param-editor", device.id));
        ui.push_id(scope, |ui| {
            self.plugin_param_editor_inner(ui, device, spec);
        });
    }

    fn plugin_param_editor_inner(
        &mut self,
        ui: &mut egui::Ui,
        device: &DeviceInstance,
        spec: &notwig_core::dsp::DeviceSpec,
    ) {
        let mut search = self
            .plugin_search
            .get(&device.id)
            .cloned()
            .unwrap_or_default();
        if plugin_search_bar(ui, &mut search) {
            self.plugin_search.insert(device.id, search.clone());
        }
        ui.add_space(4.0);

        let needle = search.to_lowercase();
        let params: Vec<&ParamSpec> = spec
            .params
            .iter()
            .filter(|param| param.flags.visible)
            .filter(|param| needle.is_empty() || param.name.to_lowercase().contains(&needle))
            .collect();
        let last = self.last_touched_param.get(&device.id).copied();
        let pinned = last.and_then(|id| params.iter().find(|param| param.id == id).copied());
        if let Some(param) = pinned {
            self.plugin_param_row(ui, device, param, true);
        }
        let rows: Vec<&ParamSpec> = params;
        if rows.is_empty() {
            if pinned.is_none() {
                ui.label(
                    RichText::new("No matching parameters.")
                        .small()
                        .color(theme::TEXT_MUTED),
                );
            }
            return;
        }
        ui.spacing_mut().item_spacing.y = 0.0;
        egui::ScrollArea::vertical()
            .id_salt(ui.make_persistent_id(("plugin-param-list", device.id)))
            .auto_shrink([false, false])
            .show_rows(ui, PARAM_ROW_H, rows.len(), |ui, range| {
                for index in range {
                    self.plugin_param_row(ui, device, rows[index], false);
                }
            });
    }

    fn plugin_param_row(
        &mut self,
        ui: &mut egui::Ui,
        device: &DeviceInstance,
        param: &ParamSpec,
        pinned: bool,
    ) {
        let width = ui.available_width().max(160.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, PARAM_ROW_H), Sense::hover());
        let hovered = ui.rect_contains_pointer(rect);
        let painter = ui.painter().clone();
        if pinned {
            painter.rect_filled(
                rect,
                CornerRadius::same(4),
                theme::ACCENT_DIM.gamma_multiply(0.55),
            );
        } else if hovered {
            painter.rect_filled(rect, CornerRadius::same(4), theme::GRID_STRONG);
        }
        painter.line_segment(
            [
                egui::pos2(rect.left() + 6.0, rect.bottom()),
                egui::pos2(rect.right() - 6.0, rect.bottom()),
            ],
            Stroke::new(1.0_f32, theme::GRID),
        );

        let current = device
            .param_values
            .get(&param.id)
            .cloned()
            .unwrap_or_else(|| param.default.clone());
        let norm = param.norm(&current).0;
        let bipolar = param.range.min < 0.0 && param.range.max > 0.0;
        let knob_rect = Rect::from_center_size(
            egui::pos2(rect.left() + 27.0, rect.center().y),
            Vec2::splat(42.0),
        );
        let default_norm = param.norm(&param.default).0;
        if let Some(norm) = crate::widgets::knob(
            ui,
            ui.make_persistent_id(("plugin-param-knob", device.id, param.id)),
            knob_rect,
            norm,
            default_norm,
            bipolar,
            &param.name,
        ) {
            self.last_touched_param.insert(device.id, param.id);
            self.emit(UiIntent::SetParam {
                device_id: device.id,
                param_id: param.id,
                value: param.value_from_norm(Norm01::clamped(norm)),
            });
        }

        let text_left = knob_rect.right() + 6.0;
        let text_rect = Rect::from_min_max(
            egui::pos2(text_left, rect.top() + 6.0),
            egui::pos2(rect.right() - 8.0, rect.bottom() - 4.0),
        );
        let text_painter = painter.with_clip_rect(text_rect);
        text_painter.text(
            egui::pos2(text_left, rect.top() + 7.0),
            Align2::LEFT_TOP,
            &param.name,
            FontId::proportional(12.5),
            if pinned {
                Color32::WHITE
            } else {
                theme::TEXT_MUTED
            },
        );
        text_painter.text(
            egui::pos2(text_left, rect.top() + 25.0),
            Align2::LEFT_TOP,
            plugin_value_text(param, &current),
            FontId::monospace(11.0),
            Color32::WHITE,
        );
    }
}

fn plugin_search_bar(ui: &mut egui::Ui, search: &mut String) -> bool {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().max(160.0), 27.0),
        Sense::hover(),
    );
    let painter = ui.painter().clone();
    painter.rect_filled(rect, CornerRadius::same(5), Color32::from_rgb(18, 19, 23));
    painter.rect_stroke(
        rect,
        CornerRadius::same(5),
        Stroke::new(1.0_f32, theme::GRID),
        egui::StrokeKind::Inside,
    );
    let icon_center = egui::pos2(rect.left() + 15.0, rect.center().y);
    crate::icons::paint(
        &painter,
        egui::Rect::from_center_size(icon_center, Vec2::splat(14.0)),
        crate::icons::Icon::Search,
        theme::TEXT_MUTED,
    );
    let text_rect = Rect::from_min_max(
        egui::pos2(rect.left() + 29.0, rect.top() + 3.0),
        egui::pos2(
            rect.right() - if search.is_empty() { 8.0 } else { 30.0 },
            rect.bottom() - 3.0,
        ),
    );
    let response = ui.put(
        text_rect,
        egui::TextEdit::singleline(search)
            .frame(false)
            .hint_text("Search parameters")
            .desired_width(f32::INFINITY),
    );
    if !search.is_empty() {
        let clear = Rect::from_center_size(
            egui::pos2(rect.right() - 15.0, rect.center().y),
            Vec2::splat(18.0),
        );
        let clear_resp = ui.interact(
            clear,
            ui.make_persistent_id("plugin-search-clear"),
            Sense::click(),
        );
        painter.text(
            clear.center(),
            Align2::CENTER_CENTER,
            "×",
            FontId::proportional(16.0),
            if clear_resp.hovered() {
                Color32::WHITE
            } else {
                theme::TEXT_MUTED
            },
        );
        if clear_resp.clicked() {
            search.clear();
            return true;
        }
    }
    response.changed()
}

fn plugin_value_text(param: &ParamSpec, value: &ParamValue) -> String {
    match param.kind {
        ParamKind::Bool => {
            if matches!(value, ParamValue::Bool(true)) {
                "On".into()
            } else {
                "Off".into()
            }
        }
        ParamKind::Enum | ParamKind::Discrete => value
            .as_f64()
            .map_or_else(|| "0".into(), |value| format!("{}", value.round() as u32)),
        ParamKind::Continuous => format_param(param.unit, param.physical(value)),
    }
}
