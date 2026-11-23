use crate::{DeviceId, DeviceKind, NoteId, ParamId, ParamValue, SchemaVersion, TimeMus};

pub use crate::tracks::SignalType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PortDir {
    In,
    Out,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Port {
    pub id: String,
    pub dir: PortDir,
    pub signal: SignalType,
    pub name: String,
    pub optional: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Latency {
    pub samples: u32,
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub enum DeviceUiSpec {
    #[default]
    Generic,
    Custom {
        renderer: String,
        preferred_width: Option<f32>,
    },
}

impl DeviceUiSpec {
    #[must_use]
    pub fn custom(renderer: impl Into<String>, preferred_width: Option<f32>) -> Self {
        Self::Custom {
            renderer: renderer.into(),
            preferred_width,
        }
    }
}

/// How a container's child chain is positioned relative to the device stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ContainerRouting {
    /// The device stage runs first, then the child chain processes its output.
    ///
    /// Use this for devices whose child chain should process the wet signal,
    /// such as a reverb with a wet FX chain.
    #[default]
    StageThenChildren,
    /// The child chain runs first, then the device stage processes the result.
    ChildrenThenStage,
}

/// Container wiring declared by a [`DeviceSpec`] with [`DeviceKind::Container`].
///
/// The host owns the child chain. Extension devices supply only the stage,
/// parameters, and panel. `wet_param` makes the whole container a wet path:
/// the input is copied to a dry buffer, the stage and child chain process the
/// wet path, and the host crossfades between them using the parameter's
/// normalized value.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct ContainerSpec {
    /// Position of the child chain relative to the device stage.
    #[serde(default)]
    pub routing: ContainerRouting,
    /// Optional parameter that controls the container's dry/wet blend.
    ///
    /// The parameter must also appear in [`DeviceSpec::params`]. The host reads
    /// its `0..=1` value; the stage's `process` must output the wet signal only.
    #[serde(default)]
    pub wet_param: Option<ParamId>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DeviceSpec {
    pub type_id: String,
    pub version: SchemaVersion,
    pub kind: DeviceKind,
    pub ports: Vec<Port>,
    pub params: Vec<super::params::ParamSpec>,
    pub latency_samples: u32,
    pub name: String,
    pub vendor: String,
    #[serde(default)]
    pub ui: DeviceUiSpec,
    /// Container wiring for [`DeviceKind::Container`]. `None` uses defaults.
    #[serde(default)]
    pub container: Option<ContainerSpec>,
}

impl DeviceSpec {
    #[must_use]
    pub fn param(&self, id: ParamId) -> Option<&super::params::ParamSpec> {
        self.params.iter().find(|param| param.id == id)
    }

    #[must_use]
    pub fn default_value(&self, id: ParamId) -> Option<ParamValue> {
        self.param(id).map(|param| param.default.clone())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EngineCaps {
    pub sample_rate: u32,
    pub max_block_size: usize,
    pub channels: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RenderConfig {
    pub sample_rate: u32,
    pub max_block_size: usize,
    pub channels: u16,
    pub tempo_bpm: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct ProcessContext<'a> {
    pub config: RenderConfig,
    pub sample_position: u64,
    pub playing: bool,
    pub tempo_bpm: f64,
    pub transport: &'a TransportInfo,
}

#[derive(Debug, Clone, Copy)]
pub struct TransportInfo {
    pub playhead_mus: TimeMus,
    pub loop_start_mus: Option<TimeMus>,
    pub loop_end_mus: Option<TimeMus>,
}

impl Default for TransportInfo {
    fn default() -> Self {
        Self {
            playhead_mus: TimeMus::zero(),
            loop_start_mus: None,
            loop_end_mus: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteKind {
    On,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoteEvent {
    pub sample_offset: u32,
    pub kind: NoteKind,
    pub note_id: NoteId,
    pub pitch: u8,
    pub velocity: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProcessStatus {
    pub overloaded: bool,
    pub produced_silence: bool,
}

pub trait DeviceProcessor: Send {
    /// Prepares the processor for `config` before any [`Self::process`] call.
    ///
    /// This runs off the audio thread when an engine is built or reconfigured,
    /// so implementations may allocate or resize state here. They must not do
    /// that in `process`.
    fn prepare(&mut self, config: RenderConfig) -> Result<(), super::GraphError>;
    fn reset(&mut self);
    fn set_param(&mut self, param_id: ParamId, value: &ParamValue);
    /// Processes one in-place, interleaved stereo block.
    ///
    /// `audio.len()` is expected to be even and no larger than
    /// `config.max_block_size * 2` from the preceding [`Self::prepare`];
    /// implementations commonly walk it with `chunks_exact_mut(2)`. Must not
    /// allocate, block, or perform I/O on the audio thread.
    fn process(
        &mut self,
        ctx: &ProcessContext<'_>,
        audio: &mut [f32],
        events: &[NoteEvent],
    ) -> ProcessStatus;
    fn latency(&self) -> Latency {
        Latency { samples: 0 }
    }

    fn set_nested_param(
        &mut self,
        _device_id: DeviceId,
        _param_id: ParamId,
        _value: &ParamValue,
    ) -> bool {
        false
    }

    /// Toggles runtime bypass for the device named by `device_id`.
    ///
    /// Returns `true` when this processor (or one of its nested children) owns
    /// the device. The processor must stay loaded; bypass only stops audio
    /// processing.
    fn set_bypass(&mut self, _device_id: DeviceId, _bypass: bool) -> bool {
        false
    }
}
