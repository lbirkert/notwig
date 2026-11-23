//! VST 2.4 host implementation for the notwig-rack framework.
//!
//! VST 2 predates the modern plugin APIs and is a plain C ABI: a
//! plugin exports `VSTPluginMain`, returns an `AEffect` struct of
//! function pointers, and talks back to the host through a single
//! callback. This crate wraps that contract behind the
//! format-agnostic [`notwig_rack_core`] traits.
//!
//! The ABI declarations live in [`abi`] and are translated from
//! Xaymar's clean-room `vst2sdk` headers (BSD-3-Clause). See the
//! repository `NOTICE` file for the full provenance.
//!
//! VST 2 audio is always 32-bit float. Nodes are planar on the host
//! side and `processReplacing` is preferred when available.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::missing_safety_doc
)]

mod abi;
mod editor;
mod host;
mod loader;
mod midi;
mod plugin;
mod scan;
mod time;

#[cfg(test)]
mod tests;

pub use plugin::Vst2Plugin;
pub use scan::{FORMAT, VST2_EXTENSION, Vst2Scanner, default_vst2_paths};

pub(crate) const DEFAULT_SAMPLE_RATE: f64 = 48_000.0;
pub(crate) const DEFAULT_BLOCK_SIZE: usize = 512;
pub(crate) const MAX_CHANNELS: usize = 64;
pub(crate) const MAX_STATE_BYTES: usize = 64 * 1024 * 1024;

#[cfg(test)]
use abi::{AEffect, EventList as VstEventList, MAX_EVENTS, MidiEvent, Rect, TimeInfo};
#[cfg(test)]
use notwig_rack_core::events::{EventBody, MidiData};
#[cfg(test)]
use notwig_rack_core::transport::TransportInfo;
#[cfg(test)]
use std::ffi::OsStr;
#[cfg(test)]
use std::ptr;
#[cfg(test)]
use std::sync::atomic::Ordering;

#[cfg(test)]
pub(crate) use editor::rect_size;
#[cfg(test)]
pub(crate) use host::{HostShared, host_callback};
#[cfg(test)]
pub(crate) use midi::{midi_body, midi_to_bytes};
#[cfg(test)]
pub(crate) use plugin::layout_for;
#[cfg(test)]
pub(crate) use time::TimeState;
