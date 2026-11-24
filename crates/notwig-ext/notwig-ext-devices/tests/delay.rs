mod common;

use notwig_core::DeviceKind;
use notwig_ext_api::DeviceExtension;
use notwig_ext_devices::{
    DELAY_TYPE_ID, DelayDevice, delay_crossfeed_id, delay_damping_id, delay_ducking_id,
    delay_feedback_id, delay_mix_id, delay_time_left_id, delay_time_right_id, delay_width_id,
};

#[test]
fn spec_is_valid() {
    let spec = DelayDevice::spec();
    assert_eq!(spec.type_id, DELAY_TYPE_ID);
    assert_eq!(spec.name, "Delay");
    assert_eq!(spec.kind, DeviceKind::AudioFx);
    assert_eq!(spec.params.len(), 8);
    for id in [
        delay_time_left_id(),
        delay_time_right_id(),
        delay_feedback_id(),
        delay_crossfeed_id(),
        delay_damping_id(),
        delay_ducking_id(),
        delay_width_id(),
        delay_mix_id(),
    ] {
        assert!(spec.params.iter().any(|param| param.id == id));
    }
}

#[test]
fn draw_produces_a_valid_frame() {
    let mut delay = DelayDevice::new();
    let frame = common::draw_frame(380.0, 200.0, |ui, meter| delay.draw(ui, meter));
    frame.validate().unwrap();
}

#[test]
fn impulse_appears_at_the_delay_time() {
    let mut delay = DelayDevice::new();
    delay.prepare(48_000, 2);
    delay.set_param(delay_time_left_id(), 0.01);
    delay.set_param(delay_time_right_id(), 0.01);
    delay.set_param(delay_feedback_id(), 0.0);
    delay.set_param(delay_mix_id(), 1.0);
    delay.set_param(delay_width_id(), 0.0);
    let mut audio = vec![0.0_f32; 2_048 * 2];
    audio[0] = 1.0;
    audio[1] = 1.0;
    delay.process(&mut audio);
    let delayed = audio[480 * 2];
    assert!(
        delayed > 0.5,
        "impulse did not arrive at the delay time: {delayed}"
    );
    assert!(audio[479 * 2].abs() < 1e-4);
}

#[test]
fn feedback_stays_bounded() {
    let mut delay = DelayDevice::new();
    delay.prepare(48_000, 2);
    delay.set_param(delay_feedback_id(), 0.95);
    delay.set_param(delay_mix_id(), 1.0);
    let mut audio = vec![0.0_f32; 48_000 * 2];
    audio[0] = 1.0;
    audio[1] = 1.0;
    delay.process(&mut audio);
    assert!(
        audio
            .iter()
            .all(|sample| sample.is_finite() && sample.abs() < 4.0)
    );
}

#[test]
fn unknown_parameters_are_rejected() {
    let mut delay = DelayDevice::new();
    assert!(!delay.set_param(notwig_core::ParamId::nil(), 1.0));
}
