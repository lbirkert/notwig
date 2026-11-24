use egui::{Align2, Color32, FontId, Sense, Stroke, Vec2};
use notwig_core::dsp::DeviceRegistry;
use notwig_core::{Color, DeviceChainId, ProjectDoc};

use crate::theme;

pub(super) fn section_header(ui: &mut egui::Ui, label: &str) {
    ui.label(theme::heading(ui.ctx(), label));
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

/// Renders the shared color swatches. The final "inherit" swatch clears an
/// explicit color. Returns `None` when nothing was clicked.
pub(super) fn color_swatch_row(ui: &mut egui::Ui, current: Option<Color>) -> Option<Option<Color>> {
    let mut picked = None;
    let spacing = ui.spacing().item_spacing;
    ui.spacing_mut().item_spacing = Vec2::new(2.0, 2.0);
    ui.horizontal_wrapped(|ui| {
        for rgb in TRACK_COLORS {
            let color = Color {
                r: rgb[0],
                g: rgb[1],
                b: rgb[2],
                a: 255,
            };
            let (rect, response) = ui.allocate_exact_size(Vec2::new(18.0, 16.0), Sense::click());
            let fill = Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a);
            ui.painter()
                .rect_filled(rect, egui::CornerRadius::same(2), fill);
            if current == Some(color) {
                ui.painter().rect_stroke(
                    rect,
                    egui::CornerRadius::same(2),
                    Stroke::new(2.0_f32, Color32::WHITE),
                    egui::StrokeKind::Inside,
                );
            }
            if response.clicked() {
                picked = Some(Some(color));
            }
        }
        let (rect, response) = ui.allocate_exact_size(Vec2::new(18.0, 16.0), Sense::click());
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(2),
            Color32::from_rgb(22, 24, 29),
        );
        if current.is_none() {
            ui.painter().rect_stroke(
                rect,
                egui::CornerRadius::same(2),
                Stroke::new(2.0_f32, Color32::WHITE),
                egui::StrokeKind::Inside,
            );
        }
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            "×",
            FontId::proportional(12.0),
            theme::TEXT_MUTED,
        );
        if response.clicked() {
            picked = Some(None);
        }
    });
    ui.spacing_mut().item_spacing = spacing;
    picked
}

/// Total processing latency of a device chain, including nested child chains.
///
/// Inactive devices are skipped because they are not instantiated. Bypassed
/// devices keep their planned latency so toggling bypass does not move the
/// latency-compensation baseline.
pub(super) fn chain_latency(
    doc: &ProjectDoc,
    registry: &DeviceRegistry,
    chain_id: DeviceChainId,
) -> u32 {
    let Some(chain) = doc.chains.get(&chain_id) else {
        return 0;
    };
    let mut total = 0u32;
    for id in &chain.devices {
        let Some(device) = doc.devices.get(id) else {
            continue;
        };
        if !device.active {
            continue;
        }
        if let Some(spec) = registry.resolve(&device.type_id, device.version) {
            total = total.saturating_add(spec.latency_samples);
        }
        for child in &device.child_chains {
            total = total.saturating_add(chain_latency(doc, registry, *child));
        }
    }
    total
}

pub(super) fn latency_label(samples: u32, sample_rate: u32) -> String {
    if sample_rate == 0 {
        return format!("{samples} samples");
    }
    let ms = f64::from(samples) * 1000.0 / f64::from(sample_rate);
    if ms >= 10.0 {
        format!("{ms:.0} ms")
    } else if ms > 0.0 {
        format!("{ms:.2} ms")
    } else {
        "0 ms".to_owned()
    }
}
