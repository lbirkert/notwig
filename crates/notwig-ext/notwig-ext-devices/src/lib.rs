//! Stock extension devices bundled with Notwig.
//!
//! This crate implements the built-in limiter, gate, synth, reverb, and gain
//! devices on top of `notwig-ext-api` and exposes them through the [`extension`]
//! entrypoint. It doubles as a worked example of the extension ABI, device
//! specs, panels, containers, and real-time processing rules.

use notwig_ext_api::{Extension, notwig_extension};

mod dynamics;
mod gain;
mod gate;
mod limiter;
mod reverb;
mod synth;
mod ui;

pub use gain::{GainDevice, TYPE_ID as GAIN_TYPE_ID, gain_id};

pub use gate::{
    GateDevice, TYPE_ID as GATE_TYPE_ID, attack_id as gate_attack_id, hold_id as gate_hold_id,
    range_id as gate_range_id, release_id as gate_release_id, threshold_id as gate_threshold_id,
};
pub use limiter::{LOOKAHEAD_SAMPLES, LimiterDevice, TYPE_ID, release_id, threshold_id};
pub use reverb::{
    ReverbDevice, TYPE_ID as REVERB_TYPE_ID, damping_id as reverb_damping_id,
    mix_id as reverb_mix_id, predelay_id as reverb_predelay_id, size_id as reverb_size_id,
    width_id as reverb_width_id,
};
pub use synth::{
    SYNTH_TYPE_ID, SynthDevice, attack_id as synth_attack_id, decay_id as synth_decay_id,
    oscillator_id as synth_oscillator_id, release_id as synth_release_id,
    sustain_id as synth_sustain_id,
};

/// Registers every stock device implemented by this crate.
#[notwig_extension]
pub fn extension(extension: &mut Extension) {
    extension.register_device::<LimiterDevice>();
    extension.register_device::<GateDevice>();
    extension.register_device::<ReverbDevice>();
    extension.register_device::<SynthDevice>();
    extension.register_device::<GainDevice>();
}
