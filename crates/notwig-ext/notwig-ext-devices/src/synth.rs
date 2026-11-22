use egui::{Align2, FontId, Sense, Vec2};
use notwig_core::dsp::{DeviceSpec, DeviceUiSpec, ParamSpec, Port, PortDir, SignalType, Unit};
use notwig_core::{DeviceKind, ParamId, ParamValue, SchemaVersion};
use notwig_ext_api::{
    DeviceExtension, DeviceMeterSnapshot, NoteEvent, NoteKind, egui, panel_frame,
};
use notwig_ui::{dropdown, knob, palette, section};
use uuid::Uuid;

pub const TYPE_ID: &str = "notwig.ext.devices.synth";

/// Public alias used by the crate root export.
pub const SYNTH_TYPE_ID: &str = TYPE_ID;

const OSCILLATOR_UUID: u128 = 0x5359_4e54_4800_0000_0000_0000_0000_0001;
const ATTACK_UUID: u128 = 0x5359_4e54_4800_0000_0000_0000_0000_0002;
const DECAY_UUID: u128 = 0x5359_4e54_4800_0000_0000_0000_0000_0003;
const SUSTAIN_UUID: u128 = 0x5359_4e54_4800_0000_0000_0000_0000_0004;
const RELEASE_UUID: u128 = 0x5359_4e54_4800_0000_0000_0000_0000_0005;

const DEFAULT_OSCILLATOR: u32 = 0;
const DEFAULT_ATTACK_SECONDS: f64 = 0.01;
const DEFAULT_DECAY_SECONDS: f64 = 0.2;
const DEFAULT_SUSTAIN: f64 = 0.7;
const DEFAULT_RELEASE_SECONDS: f64 = 0.2;

const MIN_ATTACK_SECONDS: f64 = 0.001;
const MAX_ATTACK_SECONDS: f64 = 2.0;
const MIN_DECAY_SECONDS: f64 = 0.001;
const MAX_DECAY_SECONDS: f64 = 2.0;
const MIN_RELEASE_SECONDS: f64 = 0.001;
const MAX_RELEASE_SECONDS: f64 = 4.0;

const MAX_VOICES: usize = 16;
const MAX_EVENTS: usize = 256;
const VOICE_GAIN: f32 = 0.22;

const OSCILLATOR_OPTIONS: [&str; 4] = ["Sine", "Square", "Saw", "Triangle"];

#[must_use]
pub fn oscillator_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(OSCILLATOR_UUID))
}

#[must_use]
pub fn attack_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(ATTACK_UUID))
}

#[must_use]
pub fn decay_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(DECAY_UUID))
}

#[must_use]
pub fn sustain_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(SUSTAIN_UUID))
}

#[must_use]
pub fn release_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(RELEASE_UUID))
}

fn attack_norm(seconds: f64) -> f32 {
    ((seconds - MIN_ATTACK_SECONDS) / (MAX_ATTACK_SECONDS - MIN_ATTACK_SECONDS)) as f32
}

fn decay_norm(seconds: f64) -> f32 {
    ((seconds - MIN_DECAY_SECONDS) / (MAX_DECAY_SECONDS - MIN_DECAY_SECONDS)) as f32
}

fn release_norm(seconds: f64) -> f32 {
    ((seconds - MIN_RELEASE_SECONDS) / (MAX_RELEASE_SECONDS - MIN_RELEASE_SECONDS)) as f32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum OscillatorMode {
    #[default]
    Sine,
    Square,
    Saw,
    Triangle,
}

impl OscillatorMode {
    fn from_index(index: u32) -> Self {
        match index {
            0 => Self::Sine,
            1 => Self::Square,
            2 => Self::Saw,
            _ => Self::Triangle,
        }
    }

    fn index(self) -> u32 {
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
struct Voice {
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

#[derive(Debug, Clone)]
pub struct SynthDevice {
    sample_rate: f64,
    channels: usize,
    oscillator: OscillatorMode,
    attack_seconds: f64,
    decay_seconds: f64,
    sustain: f64,
    release_seconds: f64,
    voices: [Voice; MAX_VOICES],
    pending: [NoteEvent; MAX_EVENTS],
    pending_len: usize,
    next_voice: usize,
}

impl Default for SynthDevice {
    fn default() -> Self {
        Self {
            sample_rate: 48_000.0,
            channels: 2,
            oscillator: OscillatorMode::from_index(DEFAULT_OSCILLATOR),
            attack_seconds: DEFAULT_ATTACK_SECONDS,
            decay_seconds: DEFAULT_DECAY_SECONDS,
            sustain: DEFAULT_SUSTAIN,
            release_seconds: DEFAULT_RELEASE_SECONDS,
            voices: [Voice::default(); MAX_VOICES],
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

impl DeviceExtension for SynthDevice {
    fn new() -> Self {
        Self::default()
    }

    fn spec() -> DeviceSpec {
        DeviceSpec {
            type_id: TYPE_ID.into(),
            version: SchemaVersion::V0,
            kind: DeviceKind::Instrument,
            ports: vec![
                Port {
                    id: "notes".into(),
                    dir: PortDir::In,
                    signal: SignalType::NoteEvent,
                    name: "Notes".into(),
                    optional: true,
                },
                Port {
                    id: "out".into(),
                    dir: PortDir::Out,
                    signal: SignalType::Audio(2),
                    name: "Output".into(),
                    optional: false,
                },
            ],
            params: vec![
                ParamSpec::enumeration(oscillator_id(), "Oscillator", 4, DEFAULT_OSCILLATOR),
                ParamSpec::continuous(
                    attack_id(),
                    "Attack",
                    Unit::Seconds,
                    MIN_ATTACK_SECONDS,
                    MAX_ATTACK_SECONDS,
                    DEFAULT_ATTACK_SECONDS,
                ),
                ParamSpec::continuous(
                    decay_id(),
                    "Decay",
                    Unit::Seconds,
                    MIN_DECAY_SECONDS,
                    MAX_DECAY_SECONDS,
                    DEFAULT_DECAY_SECONDS,
                ),
                ParamSpec::continuous(
                    sustain_id(),
                    "Sustain",
                    Unit::Percent,
                    0.0,
                    1.0,
                    DEFAULT_SUSTAIN,
                ),
                ParamSpec::continuous(
                    release_id(),
                    "Release",
                    Unit::Seconds,
                    MIN_RELEASE_SECONDS,
                    MAX_RELEASE_SECONDS,
                    DEFAULT_RELEASE_SECONDS,
                ),
            ],
            latency_samples: 0,
            name: "Synth".into(),
            vendor: "Notwig".into(),
            ui: DeviceUiSpec::custom(format!("ext:{TYPE_ID}"), Some(260.0)),
        }
    }

    fn draw(&mut self, ui: &mut egui::Ui, _meter: DeviceMeterSnapshot) {
        section(ui, "Synth");

        let width = ui.available_width();
        let selected = self.oscillator.index() as usize;
        if let Some(index) = dropdown(
            ui,
            ui.make_persistent_id(("ext-synth", "oscillator")),
            selected,
            &OSCILLATOR_OPTIONS,
        ) {
            self.oscillator = OscillatorMode::from_index(index as u32);
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
                    attack_norm(self.attack_seconds),
                    attack_norm(DEFAULT_ATTACK_SECONDS),
                ),
                (
                    "Dec",
                    decay_norm(self.decay_seconds),
                    decay_norm(DEFAULT_DECAY_SECONDS),
                ),
                ("Sus", self.sustain as f32, DEFAULT_SUSTAIN as f32),
                (
                    "Rel",
                    release_norm(self.release_seconds),
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
                ui.painter().with_clip_rect(rect).text(
                    egui::pos2(rect.center().x, rect.bottom() + 8.0),
                    Align2::CENTER_TOP,
                    label,
                    FontId::proportional(10.0),
                    palette::TEXT_MUTED,
                );
            }

            if let Some(norm) = changes[0] {
                self.attack_seconds = MIN_ATTACK_SECONDS
                    + f64::from(norm) * (MAX_ATTACK_SECONDS - MIN_ATTACK_SECONDS);
                panel_frame::set_param_normalized(ui, attack_id(), norm);
            }
            if let Some(norm) = changes[1] {
                self.decay_seconds =
                    MIN_DECAY_SECONDS + f64::from(norm) * (MAX_DECAY_SECONDS - MIN_DECAY_SECONDS);
                panel_frame::set_param_normalized(ui, decay_id(), norm);
            }
            if let Some(norm) = changes[2] {
                self.sustain = f64::from(norm);
                panel_frame::set_param_normalized(ui, sustain_id(), norm);
            }
            if let Some(norm) = changes[3] {
                self.release_seconds = MIN_RELEASE_SECONDS
                    + f64::from(norm) * (MAX_RELEASE_SECONDS - MIN_RELEASE_SECONDS);
                panel_frame::set_param_normalized(ui, release_id(), norm);
            }
        });

        ui.ctx().request_repaint();
    }

    fn prepare(&mut self, sample_rate: u32, channels: u16) {
        self.sample_rate = f64::from(sample_rate.max(1));
        self.channels = usize::from(channels.max(1));
        self.reset_voices();
    }

    fn set_param(&mut self, param_id: ParamId, value: f64) -> bool {
        if param_id == oscillator_id() {
            self.oscillator = OscillatorMode::from_index(value.round().clamp(0.0, 3.0) as u32);
            true
        } else if param_id == attack_id() {
            self.attack_seconds = value.clamp(MIN_ATTACK_SECONDS, MAX_ATTACK_SECONDS);
            true
        } else if param_id == decay_id() {
            self.decay_seconds = value.clamp(MIN_DECAY_SECONDS, MAX_DECAY_SECONDS);
            true
        } else if param_id == sustain_id() {
            self.sustain = value.clamp(0.0, 1.0);
            true
        } else if param_id == release_id() {
            self.release_seconds = value.clamp(MIN_RELEASE_SECONDS, MAX_RELEASE_SECONDS);
            true
        } else {
            false
        }
    }

    fn note_event(&mut self, event: NoteEvent) {
        if self.pending_len < MAX_EVENTS {
            self.pending[self.pending_len] = event;
            self.pending_len += 1;
        }
    }

    fn process(&mut self, samples: &mut [f32]) {
        let channels = self.channels.max(1);
        let frames = samples.len() / channels;
        if frames == 0 {
            self.pending_len = 0;
            return;
        }
        let (attack_step, decay_step, sustain, release_step) = self.envelope_steps();
        let oscillator = self.oscillator;
        let sample_rate = self.sample_rate.max(1.0);
        let mut event_index = 0;
        for frame in 0..frames {
            while event_index < self.pending_len {
                let event = self.pending[event_index];
                if event.sample_offset as usize > frame {
                    break;
                }
                match event.kind {
                    NoteKind::On => self.trigger_note(event.pitch, event.velocity),
                    NoteKind::Off => self.release_note(event.pitch),
                }
                event_index += 1;
            }

            let mut mixed = 0.0_f32;
            for voice in &mut self.voices {
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
        self.pending_len = 0;
    }

    fn latency(&self) -> u32 {
        0
    }
}
