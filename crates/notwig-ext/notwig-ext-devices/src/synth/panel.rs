//! Synth panel drawing.

use egui::{Sense, Vec2};
use notwig_core::ParamValue;
use notwig_ext_api::{DeviceMeterSnapshot, egui, panel_frame};
use notwig_ui::{dropdown, knob, section};

use super::SynthDevice;
use super::params::{
    DEFAULT_ATTACK_SECONDS, DEFAULT_DECAY_SECONDS, DEFAULT_RELEASE_SECONDS, DEFAULT_SUSTAIN,
    OSCILLATOR_OPTIONS, attack_id, attack_norm, attack_seconds, decay_id, decay_norm,
    decay_seconds, oscillator_id, release_id, release_norm, release_seconds, sustain_id,
};
use crate::synth::dsp::OscillatorMode;
use crate::ui::knob_label;

pub(super) fn draw(device: &mut SynthDevice, ui: &mut egui::Ui, _meter: DeviceMeterSnapshot) {
    section(ui, "Synth");

    let width = ui.available_width();
    let selected = device.oscillator.index() as usize;
    if let Some(index) = dropdown(
        ui,
        ui.make_persistent_id(("ext-synth", "oscillator")),
        selected,
        &OSCILLATOR_OPTIONS,
    ) {
        device.oscillator = OscillatorMode::from_index(index as u32);
        panel_frame::set_param_value(ui, oscillator_id(), ParamValue::EnumIndex(index as u32));
    }

    ui.add_space(4.0);
    let gap = 2.0;
    let knob_width = ((width - gap * 3.0) / 4.0).max(24.0);
    let knob_height = 58.0;
    ui.horizontal(|ui| {
        let cells = [
            (
                "Atk",
                attack_norm(device.attack_seconds),
                attack_norm(DEFAULT_ATTACK_SECONDS),
            ),
            (
                "Dec",
                decay_norm(device.decay_seconds),
                decay_norm(DEFAULT_DECAY_SECONDS),
            ),
            ("Sus", device.sustain as f32, DEFAULT_SUSTAIN as f32),
            (
                "Rel",
                release_norm(device.release_seconds),
                release_norm(DEFAULT_RELEASE_SECONDS),
            ),
        ];
        let mut changes = [None; 4];
        for (index, (label, value, default)) in cells.into_iter().enumerate() {
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(knob_width, knob_height), Sense::hover());
            changes[index] = knob(
                ui,
                ui.make_persistent_id(("ext-synth", index)),
                rect,
                value,
                default,
                false,
                label,
            );
            knob_label(ui, rect, label);
        }

        if let Some(norm) = changes[0] {
            device.attack_seconds = attack_seconds(norm);
            panel_frame::set_param_normalized(ui, attack_id(), norm);
        }
        if let Some(norm) = changes[1] {
            device.decay_seconds = decay_seconds(norm);
            panel_frame::set_param_normalized(ui, decay_id(), norm);
        }
        if let Some(norm) = changes[2] {
            device.sustain = f64::from(norm);
            panel_frame::set_param_normalized(ui, sustain_id(), norm);
        }
        if let Some(norm) = changes[3] {
            device.release_seconds = release_seconds(norm);
            panel_frame::set_param_normalized(ui, release_id(), norm);
        }
    });

    ui.ctx().request_repaint();
}
