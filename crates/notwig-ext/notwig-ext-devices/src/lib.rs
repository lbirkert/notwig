//! Stock extension devices bundled with Notwig.
//!
//! Device implementations live in [`devices`]. This crate root only wires the
//! public surface and keeps the lifecycle entrypoints last.

mod devices;

pub use devices::*;
pub(crate) use devices::{dynamics, synth, ui, util};

use notwig_ext_api::{EP_NOTWIG_DEVICES, ExtensionBuilder, UnloadContext, notwig_ext};

#[notwig_ext(load)]
pub fn load(extension: &mut ExtensionBuilder) {
    extension.register_extension_factory::<devices::CompressorDevice>(EP_NOTWIG_DEVICES);
    extension.register_extension_factory::<devices::ClipperDevice>(EP_NOTWIG_DEVICES);
    extension.register_extension_factory::<devices::DelayDevice>(EP_NOTWIG_DEVICES);
    extension.register_extension_factory::<devices::ChorusDevice>(EP_NOTWIG_DEVICES);
    extension.register_extension_factory::<devices::TransientDevice>(EP_NOTWIG_DEVICES);
    extension.register_extension_factory::<devices::LimiterDevice>(EP_NOTWIG_DEVICES);
    extension.register_extension_factory::<devices::GateDevice>(EP_NOTWIG_DEVICES);
    extension.register_extension_factory::<devices::ReverbDevice>(EP_NOTWIG_DEVICES);
    extension.register_extension_factory::<devices::SynthDevice>(EP_NOTWIG_DEVICES);
    extension.register_extension_factory::<devices::GainDevice>(EP_NOTWIG_DEVICES);
}

#[notwig_ext(unload)]
pub fn unload(_context: &mut UnloadContext) {}
