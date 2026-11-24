//! Stock extension devices bundled with Notwig.
//!
//! This crate implements the bundled effects and instruments on top of
//! `notwig-ext-api` and exposes them through the [`extension`] entrypoint. It
//! doubles as a worked example of the extension ABI, device specs, panels,
//! containers, and real-time processing rules.

use notwig_ext_api::{Extension, notwig_extension};

mod chorus;
mod clipper;
mod compressor;
mod delay;
mod dynamics;
mod gain;
mod gate;
mod limiter;
mod reverb;
mod synth;
mod transient;
mod ui;
mod util;

pub use chorus::{
    ChorusDevice, TYPE_ID as CHORUS_TYPE_ID, delay_id as chorus_delay_id,
    depth_id as chorus_depth_id, feedback_id as chorus_feedback_id, mix_id as chorus_mix_id,
    mode_id as chorus_mode_id, rate_id as chorus_rate_id, width_id as chorus_width_id,
};
pub use clipper::{
    ClipperDevice, TYPE_ID as CLIPPER_TYPE_ID, drive_id as clipper_drive_id,
    knee_id as clipper_knee_id, mix_id as clipper_mix_id, output_id as clipper_output_id,
    tilt_id as clipper_tilt_id,
};
pub use compressor::{
    CompressorDevice, TYPE_ID as COMPRESSOR_TYPE_ID, attack_id as compressor_attack_id,
    knee_id as compressor_knee_id, makeup_id as compressor_makeup_id, mix_id as compressor_mix_id,
    ratio_id as compressor_ratio_id, release_id as compressor_release_id,
    threshold_id as compressor_threshold_id,
};
pub use delay::{
    DelayDevice, TYPE_ID as DELAY_TYPE_ID, crossfeed_id as delay_crossfeed_id,
    damping_id as delay_damping_id, ducking_id as delay_ducking_id,
    feedback_id as delay_feedback_id, mix_id as delay_mix_id, time_left_id as delay_time_left_id,
    time_right_id as delay_time_right_id, width_id as delay_width_id,
};
pub use transient::{
    TYPE_ID as TRANSIENT_TYPE_ID, TransientDevice, attack_gain_id as transient_attack_gain_id,
    attack_time_id as transient_attack_time_id, mix_id as transient_mix_id,
    mode_id as transient_mode_id, sustain_gain_id as transient_sustain_gain_id,
    sustain_time_id as transient_sustain_time_id,
};

pub use gain::{GainDevice, TYPE_ID as GAIN_TYPE_ID, gain_id};

pub use gate::{
    GateDevice, TYPE_ID as GATE_TYPE_ID, attack_id as gate_attack_id, hold_id as gate_hold_id,
    range_id as gate_range_id, release_id as gate_release_id, threshold_id as gate_threshold_id,
};
pub use limiter::{LOOKAHEAD_SAMPLES, LimiterDevice, TYPE_ID, release_id, threshold_id};
pub use reverb::{
    ReverbDevice, TYPE_ID as REVERB_TYPE_ID, damping_id as reverb_damping_id,
    mix_id as reverb_mix_id, predelay_id as reverb_predelay_id, reverb_type_id,
    size_id as reverb_size_id, width_id as reverb_width_id,
};
pub use synth::{
    SYNTH_TYPE_ID, SynthDevice, attack_id as synth_attack_id, decay_id as synth_decay_id,
    oscillator_id as synth_oscillator_id, release_id as synth_release_id,
    sustain_id as synth_sustain_id,
};

/// Registers every stock device implemented by this crate.
#[notwig_extension]
pub fn extension(extension: &mut Extension) {
    extension.register_device::<CompressorDevice>();
    extension.register_device::<ClipperDevice>();
    extension.register_device::<DelayDevice>();
    extension.register_device::<ChorusDevice>();
    extension.register_device::<TransientDevice>();
    extension.register_device::<LimiterDevice>();
    extension.register_device::<GateDevice>();
    extension.register_device::<ReverbDevice>();
    extension.register_device::<SynthDevice>();
    extension.register_device::<GainDevice>();
}
