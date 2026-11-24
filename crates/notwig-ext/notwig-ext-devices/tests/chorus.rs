mod common;

use notwig_core::DeviceKind;
use notwig_ext_api::DeviceExtension;
use notwig_ext_devices::{
    CHORUS_TYPE_ID, ChorusDevice, chorus_delay_id, chorus_depth_id, chorus_feedback_id,
    chorus_mix_id, chorus_mode_id, chorus_rate_id, chorus_width_id,
};

#[test]
fn spec_is_valid() {
    let spec = ChorusDevice::spec();
    assert_eq!(spec.type_id, CHORUS_TYPE_ID);
    assert_eq!(spec.name, "Chorus");
    assert_eq!(spec.kind, DeviceKind::AudioFx);
    assert_eq!(spec.params.len(), 7);
    for id in [
        chorus_mode_id(),
        chorus_rate_id(),
        chorus_depth_id(),
        chorus_feedback_id(),
        chorus_delay_id(),
        chorus_width_id(),
        chorus_mix_id(),
    ] {
        assert!(spec.params.iter().any(|param| param.id == id));
    }
}

#[test]
fn draw_produces_a_valid_frame() {
    let mut chorus = ChorusDevice::new();
    let frame = common::draw_frame(360.0, 200.0, |ui, meter| chorus.draw(ui, meter));
    frame.validate().unwrap();
}

#[test]
fn impulse_appears_in_the_wet_signal() {
    let mut chorus = ChorusDevice::new();
    chorus.prepare(48_000, 2);
    chorus.set_param(chorus_depth_id(), 0.0);
    chorus.set_param(chorus_delay_id(), 0.01);
    chorus.set_param(chorus_feedback_id(), 0.0);
    chorus.set_param(chorus_width_id(), 0.0);
    chorus.set_param(chorus_mix_id(), 1.0);
    let mut audio = vec![0.0_f32; 2_048 * 2];
    audio[0] = 1.0;
    audio[1] = 1.0;
    chorus.process(&mut audio);
    assert!(
        audio[480 * 2] > 0.5,
        "chorus did not produce a delayed wet signal"
    );
}

#[test]
fn feedback_stays_bounded() {
    let mut chorus = ChorusDevice::new();
    chorus.prepare(48_000, 2);
    chorus.set_param(chorus_feedback_id(), 0.9);
    let mut audio = vec![0.0_f32; 48_000 * 2];
    audio[0] = 1.0;
    audio[1] = 1.0;
    chorus.process(&mut audio);
    assert!(
        audio
            .iter()
            .all(|sample| sample.is_finite() && sample.abs() < 4.0)
    );
}

#[test]
fn unknown_parameters_are_rejected() {
    let mut chorus = ChorusDevice::new();
    assert!(!chorus.set_param(notwig_core::ParamId::nil(), 1.0));
}
