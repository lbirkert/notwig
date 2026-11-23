//! Synth voices, envelopes, note handling, and sample processing.

use notwig_core::ParamId;
use notwig_ext_api::{NoteEvent, NoteKind};

use super::SynthDevice;
use super::params::{
    MAX_ATTACK_SECONDS, MAX_DECAY_SECONDS, MAX_RELEASE_SECONDS, MIN_ATTACK_SECONDS,
    MIN_DECAY_SECONDS, MIN_RELEASE_SECONDS, attack_id, decay_id, oscillator_id, release_id,
    sustain_id,
};
use super::{MAX_EVENTS, MAX_VOICES, VOICE_GAIN};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum OscillatorMode {
    #[default]
    Sine,
    Square,
    Saw,
    Triangle,
}

impl OscillatorMode {
    pub(crate) fn from_index(index: u32) -> Self {
        match index {
            0 => Self::Sine,
            1 => Self::Square,
            2 => Self::Saw,
            _ => Self::Triangle,
        }
    }

    pub(crate) fn index(self) -> u32 {
        match self {
            Self::Sine => 0,
            Self::Square => 1,
            Self::Saw => 2,
            Self::Triangle => 3,
        }
    }

    fn sample(self, phase: f64) -> f32 {
        let phase = phase.rem_euclid(1.0);
        match self {
            Self::Sine => (phase * std::f64::consts::TAU).sin() as f32,
            Self::Square => {
                if phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            Self::Saw => (2.0 * phase - 1.0) as f32,
            Self::Triangle => (1.0 - 4.0 * (phase - 0.5).abs()) as f32,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum EnvelopeStage {
    #[default]
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Voice {
    pitch: u8,
    velocity: f32,
    phase: f64,
    frequency: f64,
    stage: EnvelopeStage,
    level: f32,
}

impl Voice {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn note_on(&mut self, pitch: u8, velocity: f32) {
        self.pitch = pitch;
        self.velocity = velocity.clamp(0.0, 1.0);
        self.phase = 0.0;
        self.frequency = 440.0 * 2.0_f64.powf((f64::from(pitch) - 69.0) / 12.0);
        self.stage = EnvelopeStage::Attack;
        self.level = 0.0;
    }

    fn note_off(&mut self) {
        if self.stage != EnvelopeStage::Idle {
            self.stage = EnvelopeStage::Release;
        }
    }

    fn is_idle(&self) -> bool {
        self.stage == EnvelopeStage::Idle
    }

    fn advance_envelope(
        &mut self,
        attack_step: f32,
        decay_step: f32,
        sustain: f32,
        release_step: f32,
    ) -> f32 {
        if self.stage == EnvelopeStage::Idle {
            return 0.0;
        }
        match self.stage {
            EnvelopeStage::Idle => {}
            EnvelopeStage::Attack => {
                self.level += attack_step;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = EnvelopeStage::Decay;
                }
            }
            EnvelopeStage::Decay => {
                self.level -= decay_step;
                if self.level <= sustain {
                    self.level = sustain;
                    self.stage = EnvelopeStage::Sustain;
                }
            }
            EnvelopeStage::Sustain => {
                self.level = sustain;
            }
            EnvelopeStage::Release => {
                self.level -= release_step;
                if self.level <= 0.0 {
                    self.level = 0.0;
                    self.stage = EnvelopeStage::Idle;
                }
            }
        }
        self.level
    }
}

impl SynthDevice {
    fn reset_voices(&mut self) {
        for voice in &mut self.voices {
            voice.reset();
        }
        self.pending_len = 0;
        self.next_voice = 0;
    }

    fn trigger_note(&mut self, pitch: u8, velocity: f32) {
        let existing = self
            .voices
            .iter()
            .position(|voice| !voice.is_idle() && voice.pitch == pitch);
        let index = existing.unwrap_or_else(|| {
            let index = self.next_voice % MAX_VOICES;
            self.next_voice = (self.next_voice + 1) % MAX_VOICES;
            index
        });
        self.voices[index].note_on(pitch, velocity);
    }

    fn release_note(&mut self, pitch: u8) {
        for voice in &mut self.voices {
            if !voice.is_idle() && voice.pitch == pitch {
                voice.note_off();
            }
        }
    }

    fn envelope_steps(&self) -> (f32, f32, f32, f32) {
        let sample_rate = self.sample_rate.max(1.0);
        let attack_samples = (self.attack_seconds * sample_rate).max(1.0);
        let decay_samples = (self.decay_seconds * sample_rate).max(1.0);
        let release_samples = (self.release_seconds * sample_rate).max(1.0);
        let sustain = self.sustain.clamp(0.0, 1.0) as f32;
        let decay_step = ((1.0 - f64::from(sustain)) / decay_samples) as f32;
        (
            (1.0 / attack_samples) as f32,
            decay_step,
            sustain,
            (1.0 / release_samples) as f32,
        )
    }
}

pub(super) fn prepare(device: &mut SynthDevice, sample_rate: u32, channels: u16) {
    device.sample_rate = f64::from(sample_rate.max(1));
    device.channels = usize::from(channels.max(1));
    device.reset_voices();
}

pub(super) fn set_param(device: &mut SynthDevice, param_id: ParamId, value: f64) -> bool {
    if param_id == oscillator_id() {
        device.oscillator = OscillatorMode::from_index(value.round().clamp(0.0, 3.0) as u32);
        true
    } else if param_id == attack_id() {
        device.attack_seconds = value.clamp(MIN_ATTACK_SECONDS, MAX_ATTACK_SECONDS);
        true
    } else if param_id == decay_id() {
        device.decay_seconds = value.clamp(MIN_DECAY_SECONDS, MAX_DECAY_SECONDS);
        true
    } else if param_id == sustain_id() {
        device.sustain = value.clamp(0.0, 1.0);
        true
    } else if param_id == release_id() {
        device.release_seconds = value.clamp(MIN_RELEASE_SECONDS, MAX_RELEASE_SECONDS);
        true
    } else {
        false
    }
}

pub(super) fn note_event(device: &mut SynthDevice, event: NoteEvent) {
    if device.pending_len < MAX_EVENTS {
        device.pending[device.pending_len] = event;
        device.pending_len += 1;
    }
}

pub(super) fn process(device: &mut SynthDevice, samples: &mut [f32]) {
    let channels = device.channels.max(1);
    let frames = samples.len() / channels;
    if frames == 0 {
        device.pending_len = 0;
        return;
    }
    let (attack_step, decay_step, sustain, release_step) = device.envelope_steps();
    let oscillator = device.oscillator;
    let sample_rate = device.sample_rate.max(1.0);
    let mut event_index = 0;
    for frame in 0..frames {
        while event_index < device.pending_len {
            let event = device.pending[event_index];
            if event.sample_offset as usize > frame {
                break;
            }
            match event.kind {
                NoteKind::On => device.trigger_note(event.pitch, event.velocity),
                NoteKind::Off => device.release_note(event.pitch),
            }
            event_index += 1;
        }

        let mut mixed = 0.0_f32;
        for voice in &mut device.voices {
            if voice.is_idle() {
                continue;
            }
            let level = voice.advance_envelope(attack_step, decay_step, sustain, release_step);
            if voice.is_idle() {
                continue;
            }
            let sample = oscillator.sample(voice.phase);
            mixed += sample * level * voice.velocity;
            voice.phase += voice.frequency / sample_rate;
            if voice.phase >= 1.0 {
                voice.phase -= voice.phase.floor();
            }
        }
        let sample = mixed * VOICE_GAIN;
        let start = frame * channels;
        for output in &mut samples[start..start + channels] {
            *output += sample;
        }
    }
    device.pending_len = 0;
}

pub(super) fn latency(_device: &SynthDevice) -> u32 {
    0
}
