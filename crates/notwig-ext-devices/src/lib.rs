use notwig_ext_api::{Extension, notwig_extension};

mod dynamics;
#[cfg(feature = "egui")]
mod egui_demo;
mod gate;
mod limiter;
mod synth;

#[cfg(feature = "egui")]
pub use egui_demo::{EguiGainDevice, TYPE_ID as EGUI_GAIN_TYPE_ID, gain_id as egui_gain_id};
pub use gate::{
    GateDevice, TYPE_ID as GATE_TYPE_ID, attack_id as gate_attack_id, hold_id as gate_hold_id,
    range_id as gate_range_id, release_id as gate_release_id, threshold_id as gate_threshold_id,
};
pub use limiter::{LOOKAHEAD_SAMPLES, LimiterDevice, TYPE_ID, release_id, threshold_id};
pub use synth::{
    SYNTH_TYPE_ID, SynthDevice, attack_id as synth_attack_id, decay_id as synth_decay_id,
    oscillator_id as synth_oscillator_id, release_id as synth_release_id,
    sustain_id as synth_sustain_id,
};

#[cfg_attr(feature = "egui", notwig_extension(egui))]
#[cfg_attr(not(feature = "egui"), notwig_extension)]
pub fn extension(extension: &mut Extension) {
    extension.register_device::<LimiterDevice>();
    extension.register_device::<GateDevice>();
    extension.register_device::<SynthDevice>();
    #[cfg(feature = "egui")]
    extension.register_device::<EguiGainDevice>();
}
