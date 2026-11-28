//! Stock subtractive synth device.
//!
//! [`params`] owns parameter ids, ranges, and normalization; [`spec`] builds
//! the device spec; [`panel`] draws the egui panel; [`dsp`] owns voices,
//! envelopes, note handling, and sample processing.

use notwig_core::ParamId;
use notwig_core::dsp::DeviceSpec;
use notwig_ext_api::{DeviceExtension, DeviceMeterSnapshot, NoteEvent, NoteKind, egui};

mod dsp;
mod panel;
mod params;
mod spec;

pub use params::{attack_id, decay_id, oscillator_id, release_id, sustain_id};

/// Device type id for the stock synth.
pub const TYPE_ID: &str = "notwig.ext.devices.synth";

/// Public alias used by the crate root export.
pub const SYNTH_TYPE_ID: &str = TYPE_ID;

const MAX_VOICES: usize = 16;
const MAX_EVENTS: usize = 256;
const VOICE_GAIN: f32 = 0.22;

/// Stock monophonic/polyphonic subtractive synth device.
#[derive(Debug, Clone, notwig_ext_api::ExtensionCall)]
#[extension_call(DeviceExtension)]
pub struct SynthDevice {
    sample_rate: f64,
    channels: usize,
    oscillator: dsp::OscillatorMode,
    attack_seconds: f64,
    decay_seconds: f64,
    sustain: f64,
    release_seconds: f64,
    voices: [dsp::Voice; MAX_VOICES],
    pending: [NoteEvent; MAX_EVENTS],
    pending_len: usize,
    next_voice: usize,
}

impl Default for SynthDevice {
    fn default() -> Self {
        Self {
            sample_rate: 48_000.0,
            channels: 2,
            oscillator: dsp::OscillatorMode::from_index(params::DEFAULT_OSCILLATOR),
            attack_seconds: params::DEFAULT_ATTACK_SECONDS,
            decay_seconds: params::DEFAULT_DECAY_SECONDS,
            sustain: params::DEFAULT_SUSTAIN,
            release_seconds: params::DEFAULT_RELEASE_SECONDS,
            voices: [dsp::Voice::default(); MAX_VOICES],
            pending: [NoteEvent {
                sample_offset: 0,
                kind: NoteKind::Off,
                pitch: 0,
                velocity: 0.0,
            }; MAX_EVENTS],
            pending_len: 0,
            next_voice: 0,
        }
    }
}

impl DeviceExtension for SynthDevice {
    fn new() -> Self {
        Self::default()
    }

    fn spec() -> DeviceSpec {
        spec::spec()
    }

    fn draw(&mut self, ui: &mut egui::Ui, meter: DeviceMeterSnapshot) {
        panel::draw(self, ui, meter);
    }

    fn prepare(&mut self, sample_rate: u32, channels: u16) {
        dsp::prepare(self, sample_rate, channels);
    }

    fn set_param(&mut self, param_id: ParamId, value: f64) -> bool {
        dsp::set_param(self, param_id, value)
    }

    fn note_event(&mut self, event: NoteEvent) {
        dsp::note_event(self, event);
    }

    fn process(&mut self, samples: &mut [f32]) {
        dsp::process(self, samples);
    }

    fn latency(&self) -> u32 {
        dsp::latency(self)
    }
}
