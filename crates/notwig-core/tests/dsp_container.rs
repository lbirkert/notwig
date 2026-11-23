use notwig_core::dsp::{
    ContainerProcessor, ContainerRouting, ContainerSpec, DeviceProcessor, DeviceSpec, GraphError,
    Latency, NoteEvent, ParamSpec, ProcessContext, ProcessStatus, RenderConfig, TransportInfo,
    Unit,
};
use notwig_core::{DeviceId, DeviceKind, ParamId, ParamValue, SchemaVersion, TimeMus};
use uuid::Uuid;

fn wet_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(0xc07a_1e00_0000_0000_0000_0000_0000_0001))
}

fn gain_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(0xc07a_1e00_0000_0000_0000_0000_0000_0002))
}

fn spec(routing: ContainerRouting, wet: Option<f64>) -> DeviceSpec {
    let mut params = Vec::new();
    if let Some(default) = wet {
        params.push(ParamSpec::continuous(
            wet_id(),
            "Mix",
            Unit::Percent,
            0.0,
            1.0,
            default,
        ));
    }
    params.push(ParamSpec::continuous(
        gain_id(),
        "Gain",
        Unit::None,
        0.0,
        4.0,
        1.0,
    ));
    DeviceSpec {
        type_id: "test.container".into(),
        version: SchemaVersion::V0,
        kind: DeviceKind::Container,
        ports: Vec::new(),
        params,
        latency_samples: 0,
        name: "Test Container".into(),
        vendor: "Test".into(),
        ui: notwig_core::dsp::DeviceUiSpec::default(),
        container: Some(ContainerSpec {
            routing,
            wet_param: wet.map(|_| wet_id()),
        }),
    }
}

#[derive(Debug, Clone, Copy)]
enum Op {
    Add(f32),
    Mul(f32),
}

#[derive(Debug, Clone, Copy)]
struct Fake {
    op: Op,
    latency: u32,
    gain: f32,
}

impl Fake {
    fn add(value: f32, latency: u32) -> Self {
        Self {
            op: Op::Add(value),
            latency,
            gain: 1.0,
        }
    }

    fn mul(value: f32, latency: u32) -> Self {
        Self {
            op: Op::Mul(value),
            latency,
            gain: 1.0,
        }
    }
}

impl DeviceProcessor for Fake {
    fn prepare(&mut self, _config: RenderConfig) -> Result<(), GraphError> {
        Ok(())
    }

    fn reset(&mut self) {}

    fn set_param(&mut self, param_id: ParamId, value: &ParamValue) {
        if param_id == gain_id() {
            self.gain = value.as_f64().unwrap_or(1.0) as f32;
        }
    }

    fn process(
        &mut self,
        _ctx: &ProcessContext<'_>,
        audio: &mut [f32],
        _events: &[NoteEvent],
    ) -> ProcessStatus {
        match self.op {
            Op::Add(value) => {
                for sample in audio {
                    *sample = (*sample + value) * self.gain;
                }
            }
            Op::Mul(value) => {
                for sample in audio {
                    *sample = *sample * value * self.gain;
                }
            }
        }
        ProcessStatus::default()
    }

    fn latency(&self) -> Latency {
        Latency {
            samples: self.latency,
        }
    }
}

fn config() -> RenderConfig {
    RenderConfig {
        sample_rate: 48_000,
        max_block_size: 64,
        channels: 2,
        tempo_bpm: 120.0,
    }
}

fn process(container: &mut ContainerProcessor, samples: &[f32]) -> Vec<f32> {
    let transport = TransportInfo {
        playhead_mus: TimeMus::zero(),
        loop_start_mus: None,
        loop_end_mus: None,
    };
    let context = ProcessContext {
        config: config(),
        sample_position: 0,
        playing: true,
        tempo_bpm: 120.0,
        transport: &transport,
    };
    let mut audio = samples.to_vec();
    container.process(&context, &mut audio, &[]);
    audio
}

#[test]
fn stage_then_children_order() {
    let mut container = ContainerProcessor::new(&spec(ContainerRouting::StageThenChildren, None));
    container.set_stage(Box::new(Fake::add(1.0, 0)));
    container.add_child(DeviceId::nil(), Box::new(Fake::mul(2.0, 0)));
    container.prepare(config()).unwrap();
    let output = process(&mut container, &[1.0, 1.0]);
    assert_eq!(output, [4.0, 4.0]);
}

#[test]
fn children_then_stage_order() {
    let mut container = ContainerProcessor::new(&spec(ContainerRouting::ChildrenThenStage, None));
    container.set_stage(Box::new(Fake::add(1.0, 0)));
    container.add_child(DeviceId::nil(), Box::new(Fake::mul(2.0, 0)));
    container.prepare(config()).unwrap();
    let output = process(&mut container, &[1.0, 1.0]);
    assert_eq!(output, [3.0, 3.0]);
}

#[test]
fn wet_parameter_blends_the_stage_and_child_chain() {
    let mut container =
        ContainerProcessor::new(&spec(ContainerRouting::StageThenChildren, Some(0.5)));
    container.set_stage(Box::new(Fake::add(1.0, 0)));
    container.add_child(DeviceId::nil(), Box::new(Fake::mul(2.0, 0)));
    container.prepare(config()).unwrap();
    let output = process(&mut container, &[1.0, 1.0]);
    // Wet path: (1 + 1) * 2 = 4. Dry: 1. Mix 0.5 -> 2.5.
    assert!((output[0] - 2.5).abs() < 1e-6);
    assert!((output[1] - 2.5).abs() < 1e-6);
}

#[test]
fn missing_stage_falls_back_to_the_child_chain() {
    let mut container =
        ContainerProcessor::new(&spec(ContainerRouting::StageThenChildren, Some(0.5)));
    container.add_child(DeviceId::nil(), Box::new(Fake::mul(2.0, 0)));
    container.prepare(config()).unwrap();
    let output = process(&mut container, &[1.0, 1.0]);
    assert_eq!(output, [2.0, 2.0]);
}

#[test]
fn nested_parameters_reach_children() {
    let child = DeviceId::nil();
    let mut container = ContainerProcessor::new(&spec(ContainerRouting::StageThenChildren, None));
    container.add_child(child, Box::new(Fake::mul(1.0, 0)));
    container.prepare(config()).unwrap();
    assert!(container.set_nested_param(child, gain_id(), &ParamValue::F64(3.0)));
    let output = process(&mut container, &[1.0, 1.0]);
    assert_eq!(output, [3.0, 3.0]);
}

#[test]
fn latency_sums_stage_and_children() {
    let mut container = ContainerProcessor::new(&spec(ContainerRouting::StageThenChildren, None));
    container.set_stage(Box::new(Fake::add(0.0, 10)));
    container.add_child(DeviceId::nil(), Box::new(Fake::mul(1.0, 5)));
    assert_eq!(container.latency().samples, 15);
}

#[test]
fn unknown_nested_parameter_is_rejected() {
    let mut container = ContainerProcessor::new(&spec(ContainerRouting::StageThenChildren, None));
    container.add_child(DeviceId::nil(), Box::new(Fake::mul(1.0, 0)));
    let other = DeviceId::from_uuid(Uuid::from_u128(0xc07a_1e00_0000_0000_0000_0000_0000_beef));
    assert!(!container.set_nested_param(other, ParamId::nil(), &ParamValue::F64(1.0)));
}
