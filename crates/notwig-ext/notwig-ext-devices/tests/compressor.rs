mod common;

use notwig_core::DeviceKind;
use notwig_ext_api::DeviceExtension;
use notwig_ext_devices::{
    COMPRESSOR_TYPE_ID, CompressorDevice, compressor_attack_id, compressor_knee_id,
    compressor_makeup_id, compressor_mix_id, compressor_ratio_id, compressor_release_id,
    compressor_threshold_id,
};

#[test]
fn spec_is_valid() {
    let spec = CompressorDevice::spec();
    assert_eq!(spec.type_id, COMPRESSOR_TYPE_ID);
    assert_eq!(spec.name, "Compressor");
    assert_eq!(spec.kind, DeviceKind::AudioFx);
    assert_eq!(spec.params.len(), 7);
    for id in [
        compressor_threshold_id(),
        compressor_ratio_id(),
        compressor_attack_id(),
        compressor_release_id(),
        compressor_knee_id(),
        compressor_makeup_id(),
        compressor_mix_id(),
    ] {
        assert!(spec.params.iter().any(|param| param.id == id));
    }
}

#[test]
fn draw_produces_a_valid_frame() {
    let mut compressor = CompressorDevice::new();
    let frame = common::draw_frame(360.0, 210.0, |ui, meter| compressor.draw(ui, meter));
    frame.validate().unwrap();
}

#[test]
fn compresses_a_loud_signal() {
    let mut compressor = CompressorDevice::new();
    compressor.prepare(48_000, 2);
    compressor.set_param(compressor_threshold_id(), -20.0);
    compressor.set_param(compressor_ratio_id(), 10.0);
    compressor.set_param(compressor_attack_id(), 0.0001);
    compressor.set_param(compressor_release_id(), 0.01);
    compressor.set_param(compressor_knee_id(), 0.0);
    let mut audio = vec![1.0_f32; 4_800 * 2];
    compressor.process(&mut audio);
    let peak = audio[4_000 * 2..]
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    assert!(peak < 0.2, "compressor did not reduce the signal: {peak}");
}

#[test]
fn leaves_quiet_signals_alone() {
    let mut compressor = CompressorDevice::new();
    compressor.prepare(48_000, 2);
    compressor.set_param(compressor_threshold_id(), -20.0);
    compressor.set_param(compressor_ratio_id(), 10.0);
    compressor.set_param(compressor_attack_id(), 0.001);
    compressor.set_param(compressor_release_id(), 0.05);
    compressor.set_param(compressor_knee_id(), 0.0);
    let mut audio = vec![0.01_f32; 4_800 * 2];
    compressor.process(&mut audio);
    let peak = audio[4_000 * 2..]
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    assert!(
        (peak - 0.01).abs() < 1e-3,
        "quiet signal was altered: {peak}"
    );
}

#[test]
fn unknown_parameters_are_rejected() {
    let mut compressor = CompressorDevice::new();
    assert!(!compressor.set_param(notwig_core::ParamId::nil(), 1.0));
}
