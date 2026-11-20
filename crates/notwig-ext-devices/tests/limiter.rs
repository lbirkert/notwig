use notwig_dsp::DeviceUiSpec;
use notwig_ext_api::{
    DeviceExtension, DeviceMeterSnapshot, DrawCommand, PanelCommand,
    geometry::{Point, Rect, Size},
    ui::{RecordUi, Style, UiContext, UiInput, UiMemory},
};
use notwig_ext_devices::{LOOKAHEAD_SAMPLES, LimiterDevice, TYPE_ID, release_id, threshold_id};

fn target() -> Rect {
    Rect::from_min_size(Point::new(0.0, 0.0), Size::new(240.0, 200.0))
}

#[test]
fn spec_is_a_valid_device_spec() {
    let spec = LimiterDevice::spec();
    assert_eq!(spec.type_id, TYPE_ID);
    assert_eq!(spec.name, "Limiter");
    assert_eq!(spec.params.len(), 2);
    assert!(spec.params.iter().any(|param| param.id == threshold_id()));
    assert!(spec.params.iter().any(|param| param.id == release_id()));
    assert_eq!(spec.latency_samples, LOOKAHEAD_SAMPLES);
    assert!(matches!(spec.ui, DeviceUiSpec::Custom { .. }));
}

#[test]
fn draw_produces_a_valid_panel() {
    let mut limiter = LimiterDevice::new();
    let input = UiInput::default();
    let mut memory = UiMemory::new();
    let panel = {
        let mut record = RecordUi::new(&input, &mut memory);
        {
            let mut ui = UiContext::new(&mut record, Style::default(), target());
            limiter.draw(&mut ui);
        }
        record.into_panel()
    };

    panel.validate().unwrap();
    assert_ne!(panel.commands.len(), 0);

    let texts: Vec<&str> = panel
        .commands
        .iter()
        .filter_map(|command| match command {
            PanelCommand::Draw(DrawCommand::Text { text, .. }) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        texts
            .iter()
            .any(|text| text.eq_ignore_ascii_case("Limiter"))
    );
    assert!(texts.contains(&"Threshold"));
    assert!(texts.contains(&"Release"));
}

#[test]
fn draw_uses_the_engine_meter_snapshot() {
    let mut limiter = LimiterDevice::new();
    let input = UiInput::default();
    let mut memory = UiMemory::new();
    let meter = DeviceMeterSnapshot {
        input_peak_l: 1.0,
        input_peak_r: 1.0,
        output_peak_l: 0.5,
        output_peak_r: 0.5,
    };
    let panel = {
        let mut record = RecordUi::new(&input, &mut memory);
        {
            let mut ui = UiContext::new_with_meter(&mut record, Style::default(), target(), meter);
            limiter.draw(&mut ui);
        }
        record.into_panel()
    };

    let texts: Vec<&str> = panel
        .commands
        .iter()
        .filter_map(|command| match command {
            PanelCommand::Draw(DrawCommand::Text { text, .. }) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(texts.contains(&"GR -6.0 dB"), "texts: {texts:?}");
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
fn limiter_looks_ahead_and_catches_an_impulse() {
    let mut limiter = LimiterDevice::new();
    limiter.prepare(48_000, 2);
    limiter.set_param(threshold_id(), -6.0);
    limiter.set_param(release_id(), 0.1);

    let impulse_frame = 100_usize;
    let mut audio = vec![0.0_f32; 1024 * 2];
    audio[impulse_frame * 2] = 8.0;
    audio[impulse_frame * 2 + 1] = 8.0;
    limiter.process(&mut audio);

    let expected = 10.0_f32.powf(-6.0 / 20.0);
    let lookahead = LOOKAHEAD_SAMPLES as usize;
    assert_eq!(limiter.latency(), LOOKAHEAD_SAMPLES);
    assert!(
        audio[..lookahead * 2].iter().all(|sample| *sample == 0.0),
        "lookahead prefix should be silent"
    );
    let output_peak = audio
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    assert!(
        output_peak <= expected + 1e-3,
        "transient peak {output_peak} exceeded {expected}"
    );
    let delayed_peak = audio[(impulse_frame + lookahead) * 2].abs();
    assert!(
        (delayed_peak - expected).abs() < 1e-3,
        "delayed transient peak {delayed_peak} did not hit {expected}"
    );
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
