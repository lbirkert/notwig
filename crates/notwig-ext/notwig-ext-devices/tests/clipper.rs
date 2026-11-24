mod common;

use notwig_core::DeviceKind;
use notwig_ext_api::DeviceExtension;
use notwig_ext_devices::{
    CLIPPER_TYPE_ID, ClipperDevice, clipper_drive_id, clipper_knee_id, clipper_mix_id,
    clipper_output_id, clipper_tilt_id,
};

#[test]
fn spec_is_valid() {
    let spec = ClipperDevice::spec();
    assert_eq!(spec.type_id, CLIPPER_TYPE_ID);
    assert_eq!(spec.name, "Clipper");
    assert_eq!(spec.kind, DeviceKind::AudioFx);
    assert_eq!(spec.params.len(), 5);
    for id in [
        clipper_drive_id(),
        clipper_knee_id(),
        clipper_tilt_id(),
        clipper_output_id(),
        clipper_mix_id(),
    ] {
        assert!(spec.params.iter().any(|param| param.id == id));
    }
}

#[test]
fn draw_produces_a_valid_frame() {
    let mut clipper = ClipperDevice::new();
    let frame = common::draw_frame(340.0, 200.0, |ui, meter| clipper.draw(ui, meter));
    frame.validate().unwrap();
}

#[test]
fn soft_clip_bounds_the_output() {
    let mut clipper = ClipperDevice::new();
    clipper.prepare(48_000, 2);
    clipper.set_param(clipper_drive_id(), 36.0);
    clipper.set_param(clipper_knee_id(), 1.0);
    clipper.set_param(clipper_output_id(), 0.0);
    let mut audio = vec![4.0_f32; 4_800 * 2];
    clipper.process(&mut audio);
    let peak = audio
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    assert!(
        peak.is_finite() && peak <= 1.01,
        "clipper output peaked at {peak}"
    );
}

#[test]
fn zero_mix_is_a_bypass() {
    let mut clipper = ClipperDevice::new();
    clipper.set_param(clipper_drive_id(), 24.0);
    clipper.set_param(clipper_mix_id(), 0.0);
    clipper.prepare(48_000, 2);
    let mut audio = vec![0.25_f32; 1_024 * 2];
    clipper.process(&mut audio);
    assert!(audio.iter().all(|sample| (*sample - 0.25).abs() < 1e-6));
}

#[test]
fn unknown_parameters_are_rejected() {
    let mut clipper = ClipperDevice::new();
    assert!(!clipper.set_param(notwig_core::ParamId::nil(), 1.0));
}
