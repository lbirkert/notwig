mod common;

use notwig_core::DeviceKind;
use notwig_ext_api::DeviceExtension;
use notwig_ext_devices::{
    TRANSIENT_TYPE_ID, TransientDevice, transient_attack_gain_id, transient_attack_time_id,
    transient_mix_id, transient_mode_id, transient_sustain_gain_id, transient_sustain_time_id,
};

#[test]
fn spec_is_valid() {
    let spec = TransientDevice::spec();
    assert_eq!(spec.type_id, TRANSIENT_TYPE_ID);
    assert_eq!(spec.name, "Transient");
    assert_eq!(spec.kind, DeviceKind::AudioFx);
    assert_eq!(spec.params.len(), 6);
    for id in [
        transient_attack_gain_id(),
        transient_attack_time_id(),
        transient_sustain_gain_id(),
        transient_sustain_time_id(),
        transient_mode_id(),
        transient_mix_id(),
    ] {
        assert!(spec.params.iter().any(|param| param.id == id));
    }
}

#[test]
fn draw_produces_a_valid_frame() {
    let mut transient = TransientDevice::new();
    let frame = common::draw_frame(360.0, 200.0, |ui, meter| transient.draw(ui, meter));
    frame.validate().unwrap();
}

#[test]
fn attack_gain_boosts_the_transient_and_sustain_gain_cuts_the_body() {
    let mut transient = TransientDevice::new();
    transient.prepare(48_000, 2);
    transient.set_param(transient_attack_gain_id(), 24.0);
    transient.set_param(transient_sustain_gain_id(), -24.0);
    transient.set_param(transient_attack_time_id(), 0.001);
    transient.set_param(transient_sustain_time_id(), 0.2);
    transient.set_param(transient_mix_id(), 1.0);
    let mut audio = vec![0.1_f32; 9_600 * 2];
    audio[0] = 1.0;
    audio[1] = 1.0;
    transient.process(&mut audio);
    let early = audio[100 * 2..200 * 2]
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    let late = audio[8_000 * 2..]
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    assert!(
        early > late * 1.5,
        "transient was not shaped: early={early}, late={late}"
    );
    assert!(audio.iter().all(|sample| sample.is_finite()));
}

#[test]
fn unknown_parameters_are_rejected() {
    let mut transient = TransientDevice::new();
    assert!(!transient.set_param(notwig_core::ParamId::nil(), 1.0));
}
