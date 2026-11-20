use notwig_dsp::DeviceUiSpec;
use notwig_ext_api::{DeviceExtension, PanelCommand};
use notwig_ext_devices::{LimiterDevice, TYPE_ID, release_id, threshold_id};

#[test]
fn spec_is_a_valid_device_spec() {
    let spec = LimiterDevice::spec();
    assert_eq!(spec.type_id, TYPE_ID);
    assert_eq!(spec.name, "Limiter");
    assert_eq!(spec.params.len(), 2);
    assert!(spec.params.iter().any(|param| param.id == threshold_id()));
    assert!(spec.params.iter().any(|param| param.id == release_id()));
    assert!(matches!(spec.ui, DeviceUiSpec::Custom { .. }));
}

#[test]
fn panel_is_valid_and_contains_knobs() {
    let panel = LimiterDevice::panel();
    panel.validate().unwrap();
    let knobs = panel
        .commands
        .iter()
        .filter(|command| matches!(command, PanelCommand::Knob { .. }))
        .count();
    assert_eq!(knobs, 2);
}

#[test]
fn limiter_limits_to_the_threshold() {
    let mut limiter = LimiterDevice::new();
    limiter.prepare(48_000, 2);
    assert!(limiter.set_param(threshold_id(), -6.0));
    let mut audio = vec![2.0_f32; 256];
    limiter.process(&mut audio);
    let peak = audio
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    let expected = 10.0_f32.powf(-6.0 / 20.0);
    assert!(peak <= expected + 1e-3, "peak {peak} exceeded {expected}");
    assert!(peak >= expected - 1e-3, "peak {peak} was below {expected}");
}

#[test]
fn limiter_releases_after_the_signal_falls() {
    let mut limiter = LimiterDevice::new();
    limiter.prepare(48_000, 2);
    limiter.set_param(threshold_id(), -6.0);
    limiter.set_param(release_id(), 0.01);
    let mut loud = vec![2.0_f32; 256];
    limiter.process(&mut loud);
    let mut quiet = vec![0.5_f32; 48_000 * 2];
    limiter.process(&mut quiet);
    let last = *quiet.last().unwrap();
    assert!(last > 0.49, "limiter did not release: {last}");
}

#[test]
fn unknown_parameters_are_rejected() {
    let mut limiter = LimiterDevice::new();
    assert!(!limiter.set_param(notwig_core::ParamId::nil(), 1.0));
}
