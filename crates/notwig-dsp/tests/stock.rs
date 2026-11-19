use notwig_core::{ParamId, ParamValue, TimeMus};
use notwig_dsp::DeviceProcessor;
use notwig_dsp::devices::{DeviceSpec, ProcessContext, RenderConfig, TransportInfo};
use notwig_dsp::params::{ParamSpec, Unit};
use notwig_dsp::stock::{
    EqProcessor, ToolProcessor, band_param, chain_spec, eq_spec, p_dry_wet, p_eq_output_gain,
    p_input_gain, p_output_gain, p_polarity_l, tool_spec,
};

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
