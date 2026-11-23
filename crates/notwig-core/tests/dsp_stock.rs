use notwig_core::dsp::DeviceProcessor;
use notwig_core::dsp::devices::{DeviceSpec, ProcessContext, RenderConfig, TransportInfo};
use notwig_core::dsp::params::{ParamSpec, Unit};
use notwig_core::dsp::stock::{
    ChainProcessor, EqProcessor, ToolProcessor, band_param, chain_spec, eq_spec, p_dry_wet,
    p_eq_output_gain, p_input_gain, p_output_gain, p_polarity_l, p_swap_lr, tool_spec,
};
use notwig_core::{ParamId, ParamValue, TimeMus};

fn config() -> RenderConfig {
    RenderConfig {
        sample_rate: 48_000,
        max_block_size: 256,
        channels: 2,
        tempo_bpm: 120.0,
    }
}

fn param(spec: &DeviceSpec, id: ParamId) -> &ParamSpec {
    spec.params
        .iter()
        .find(|param| param.id == id)
        .expect("parameter exists")
}

fn assert_db_param(spec: &DeviceSpec, id: ParamId, name: &str) {
    let param = param(spec, id);
    assert_eq!(param.name, name);
    assert_eq!(param.unit, Unit::Decibels);
    assert_eq!(param.range.min, -24.0);
    assert_eq!(param.range.max, 24.0);
    assert_eq!(param.default, ParamValue::F64(0.0));
}

fn ctx(config: RenderConfig) -> ProcessContext<'static> {
    static TRANSPORT: TransportInfo = TransportInfo {
        playhead_mus: TimeMus::zero(),
        loop_start_mus: None,
        loop_end_mus: None,
    };
    ProcessContext {
        config,
        sample_position: 0,
        playing: true,
        tempo_bpm: 120.0,
        transport: &TRANSPORT,
    }
}

#[test]
fn tool_gain_and_polarity() {
    let spec = tool_spec();
    let mut tool = ToolProcessor::new(&spec);
    tool.prepare(config()).unwrap();
    tool.set_param(p_input_gain(), &ParamValue::F64(6.0));
    tool.set_param(p_polarity_l(), &ParamValue::Bool(true));
    let mut audio = vec![0.5; 16384];
    tool.process(&ctx(config()), &mut audio, &[]);
    assert!(audio[0] > 0.0);
    assert!(audio[16382] < 0.0);
    assert!(audio[16383] > 0.0);
    assert!((audio[16382].abs() - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.01);
}

#[test]
fn tool_gain_changes_are_smoothed() {
    let spec = tool_spec();
    let mut tool = ToolProcessor::new(&spec);
    tool.prepare(config()).unwrap();
    let mut settle = vec![0.5_f32; 2048];
    tool.process(&ctx(config()), &mut settle, &[]);
    tool.set_param(p_output_gain(), &ParamValue::F64(-60.0));
    let mut first = vec![0.5_f32; 2];
    tool.process(&ctx(config()), &mut first, &[]);
    assert!(first[0].abs() > 0.3, "gain change should fade, not jump");
    let mut after = vec![0.5_f32; 48_000];
    tool.process(&ctx(config()), &mut after, &[]);
    assert!(
        after[47_998].abs() < 0.005,
        "gain should settle to the target"
    );
}

#[test]
fn tool_swap_lr_swaps_channels() {
    let spec = tool_spec();
    let swap = param(&spec, p_swap_lr());
    assert_eq!(swap.name, "Swap L/R");
    assert_eq!(swap.default, ParamValue::Bool(false));

    let mut tool = ToolProcessor::new(&spec);
    tool.prepare(config()).unwrap();
    tool.set_param(p_swap_lr(), &ParamValue::Bool(true));
    let mut audio = Vec::new();
    for _ in 0..4096 {
        audio.extend_from_slice(&[0.5, -0.5]);
    }
    tool.process(&ctx(config()), &mut audio, &[]);
    assert!(audio[8190] < 0.0, "left channel should carry the old right");
    assert!(audio[8191] > 0.0, "right channel should carry the old left");
}

#[test]
fn stock_gain_specs_keep_their_contract() {
    let tool = tool_spec();
    assert_db_param(&tool, p_input_gain(), "Input Gain");
    assert_db_param(&tool, p_output_gain(), "Output Gain");

    let eq = eq_spec();
    assert_db_param(&eq, p_eq_output_gain(), "Output Gain");

    let chain = chain_spec();
    assert_db_param(&chain, p_input_gain(), "Input Gain");
    assert_db_param(&chain, p_output_gain(), "Output Gain");
}

#[test]
fn stock_dry_wet_specs_keep_their_contract() {
    for spec in [tool_spec(), chain_spec()] {
        let param = param(&spec, p_dry_wet());
        assert_eq!(param.name, "Dry/Wet");
        assert_eq!(param.unit, Unit::Percent);
        assert_eq!(param.range.min, 0.0);
        assert_eq!(param.range.max, 1.0);
        assert_eq!(param.default, ParamValue::F64(1.0));
    }
}

#[test]
fn eq_bands_default_to_disabled() {
    let spec = eq_spec();
    for band in 0..4 {
        let enabled = spec
            .params
            .iter()
            .find(|param| param.id == band_param(band, 0))
            .expect("band enabled parameter");
        assert_eq!(enabled.default, ParamValue::Bool(false));
        let band_type = spec
            .params
            .iter()
            .find(|param| param.id == band_param(band, 1))
            .expect("band type parameter");
        assert_eq!(band_type.default, ParamValue::EnumIndex(0));
    }
}

#[test]
fn eq_bell_at_unity_is_transparent() {
    let spec = eq_spec();
    let mut eq = EqProcessor::new(&spec);
    eq.prepare(config()).unwrap();
    for band in 0..4 {
        eq.set_param(band_param(band, 0), &ParamValue::Bool(false));
    }
    let mut audio = vec![0.25, -0.25, 0.3, -0.3];
    let original = audio.clone();
    eq.process(&ctx(config()), &mut audio, &[]);
    for (a, b) in audio.iter().zip(original.iter()) {
        assert!((a - b).abs() < 1e-6);
    }
}

#[test]
fn eq_highpass_attenuates_dc() {
    let spec = eq_spec();
    let mut eq = EqProcessor::new(&spec);
    eq.prepare(config()).unwrap();
    for band in 0..4 {
        eq.set_param(band_param(band, 0), &ParamValue::Bool(false));
    }
    eq.set_param(band_param(0, 0), &ParamValue::Bool(true));
    eq.set_param(band_param(0, 1), &ParamValue::EnumIndex(3));
    eq.set_param(band_param(0, 2), &ParamValue::F64(100.0));
    eq.set_param(band_param(0, 4), &ParamValue::F64(0.707));
    let mut audio = vec![1.0; 4096];
    eq.process(&ctx(config()), &mut audio, &[]);
    assert!(audio[4090].abs() < 0.05);
}

struct Double;

impl DeviceProcessor for Double {
    fn prepare(&mut self, _config: RenderConfig) -> Result<(), notwig_core::dsp::GraphError> {
        Ok(())
    }

    fn reset(&mut self) {}

    fn set_param(&mut self, _param_id: ParamId, _value: &ParamValue) {}

    fn process(
        &mut self,
        _ctx: &ProcessContext<'_>,
        audio: &mut [f32],
        _events: &[notwig_core::dsp::NoteEvent],
    ) -> notwig_core::dsp::ProcessStatus {
        for sample in audio {
            *sample *= 2.0;
        }
        notwig_core::dsp::ProcessStatus::default()
    }
}

#[test]
fn chain_processes_oversized_blocks_without_resizing_its_dry_buffer() {
    let spec = chain_spec();
    let mut chain = ChainProcessor::new(&spec);
    chain.add_child(notwig_core::DeviceId::nil(), Box::new(Double));
    let mut small = config();
    small.max_block_size = 4;
    chain.prepare(small).unwrap();

    // The prepared dry buffer holds four stereo frames; process four times that
    // many frames to exercise the oversized defensive path.
    let mut one_go = vec![0.25_f32; 32];
    chain.process(&ctx(small), &mut one_go, &[]);

    let mut split = ChainProcessor::new(&spec);
    split.add_child(notwig_core::DeviceId::nil(), Box::new(Double));
    split.prepare(small).unwrap();
    let mut first = vec![0.25_f32; 8];
    let mut rest = vec![0.25_f32; 24];
    split.process(&ctx(small), &mut first, &[]);
    split.process(&ctx(small), &mut rest, &[]);

    let expected: Vec<f32> = first.into_iter().chain(rest).collect();
    assert_eq!(one_go, expected);
    assert!(one_go.iter().all(|sample| (*sample - 0.5).abs() < 1e-6));
}
