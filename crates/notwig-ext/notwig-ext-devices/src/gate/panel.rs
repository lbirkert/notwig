//! Gate panel drawing.

use std::time::Duration;

use egui::{Align2, FontId, Sense, Vec2};
use notwig_ext_api::{DeviceMeterSnapshot, egui, panel_frame};
use notwig_ui::{draw_stereo_meter_with_peak, knob, palette, section};

use super::GateDevice;
use super::params::{
    DEFAULT_ATTACK_SECONDS, DEFAULT_HOLD_SECONDS, DEFAULT_RANGE_DB, DEFAULT_RELEASE_SECONDS,
    DEFAULT_THRESHOLD_DB, attack_id, attack_norm, attack_seconds, hold_id, hold_norm, hold_seconds,
    range_db, range_id, range_norm, release_id, release_norm, release_seconds, threshold_db,
    threshold_id, threshold_norm,
};
use crate::dynamics::{ThresholdCurve, paint_history_graph};
use crate::ui::knob_label;

pub(super) fn draw(device: &mut GateDevice, ui: &mut egui::Ui, meter: DeviceMeterSnapshot) {
    let now = ui.input(|input| input.time);
    device.update_meters(meter, now);
    section(ui, "Gate");

    let width = ui.available_width();
    let meter_width = 40.0;
    let graph_height = 116.0;
    ui.horizontal(|ui| {
        let graph_width = (width - meter_width - 6.0).max(60.0);
        let (graph_rect, _) =
            ui.allocate_exact_size(Vec2::new(graph_width, graph_height), Sense::hover());
        paint_history_graph(
            ui.painter(),
            graph_rect,
            &device.history,
            device.threshold_db as f32,
            ThresholdCurve::Fixed,
            true,
        );
        let closed = device.gain_reduction_db < -1.0;
        let text = format!(
            "{} {:.0} dB",
            if closed { "CLOSED" } else { "OPEN" },
            device.gain_reduction_db
        );
        ui.painter().with_clip_rect(graph_rect).text(
            graph_rect.min + egui::vec2(6.0, 5.0),
            Align2::LEFT_TOP,
            text,
            FontId::monospace(9.0),
            if closed {
                palette::TEXT_MUTED
            } else {
                palette::ACCENT
            },
        );

        let (meter_rect, _) =
            ui.allocate_exact_size(Vec2::new(meter_width, graph_height), Sense::hover());
        let to_linear = |db: f32| 10.0_f32.powf(db / 20.0);
        draw_stereo_meter_with_peak(
            ui.painter(),
            meter_rect,
            to_linear(device.meter_left_db),
            to_linear(device.meter_right_db),
            Some(to_linear(device.meter_peak_db)),
        );
    });

    ui.add_space(4.0);
    let gap = 2.0;
    let knob_width = ((width - gap * 4.0) / 5.0).max(24.0);
    let knob_height = 58.0;
    ui.horizontal(|ui| {
        let cells = [
            (
                "Thr",
                threshold_norm(device.threshold_db),
                threshold_norm(DEFAULT_THRESHOLD_DB),
            ),
            (
                "Atk",
                attack_norm(device.attack_seconds),
                attack_norm(DEFAULT_ATTACK_SECONDS),
            ),
            (
                "Hold",
                hold_norm(device.hold_seconds),
                hold_norm(DEFAULT_HOLD_SECONDS),
            ),
            (
                "Rel",
                release_norm(device.release_seconds),
                release_norm(DEFAULT_RELEASE_SECONDS),
            ),
            (
                "Rng",
                range_norm(device.range_db),
                range_norm(DEFAULT_RANGE_DB),
            ),
        ];
        let mut changes = [None; 5];
        for (index, (label, value, default)) in cells.into_iter().enumerate() {
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(knob_width, knob_height), Sense::hover());
            changes[index] = knob(
                ui,
                ui.make_persistent_id(("ext-gate", index)),
                rect,
                value,
                default,
                false,
                label,
            );
            knob_label(ui, rect, label);
        }

        if let Some(norm) = changes[0] {
            device.threshold_db = threshold_db(norm);
            device.update_threshold();
            panel_frame::set_param_normalized(ui, threshold_id(), norm);
        }
        if let Some(norm) = changes[1] {
            device.attack_seconds = attack_seconds(norm);
            device.update_timing();
            panel_frame::set_param_normalized(ui, attack_id(), norm);
        }
        if let Some(norm) = changes[2] {
            device.hold_seconds = hold_seconds(norm);
            device.update_timing();
            panel_frame::set_param_normalized(ui, hold_id(), norm);
        }
        if let Some(norm) = changes[3] {
            device.release_seconds = release_seconds(norm);
            device.update_timing();
            panel_frame::set_param_normalized(ui, release_id(), norm);
        }
        if let Some(norm) = changes[4] {
            device.range_db = range_db(norm);
            device.update_range();
            panel_frame::set_param_normalized(ui, range_id(), norm);
        }
    });

    ui.ctx().request_repaint_after(Duration::from_millis(33));
}
