use notwig_core::{DeviceId, DeviceKind, NoteId, ParamId, ParamValue, SchemaVersion, TimeMus};

pub use notwig_core::tracks::SignalType;

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
    fn prepare(&mut self, config: RenderConfig) -> Result<(), super::GraphError>;
    fn reset(&mut self);
    fn set_param(&mut self, param_id: ParamId, value: &ParamValue);
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
}
