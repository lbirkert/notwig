use notwig_ext_api::{Extension, notwig_extension};

mod dynamics;
mod gate;
mod limiter;

pub use gate::{
    GateDevice, TYPE_ID as GATE_TYPE_ID, attack_id as gate_attack_id, hold_id as gate_hold_id,
    range_id as gate_range_id, release_id as gate_release_id, threshold_id as gate_threshold_id,
};
pub use limiter::{LOOKAHEAD_SAMPLES, LimiterDevice, TYPE_ID, release_id, threshold_id};

#[notwig_extension]
pub fn extension(extension: &mut Extension) {
    extension.register_device::<LimiterDevice>();
    extension.register_device::<GateDevice>();
}
