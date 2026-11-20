use notwig_dsp::DeviceUiSpec;
use notwig_ext_api::{
    DeviceExtension, DrawCommand, PanelCommand,
    geometry::{Point, Rect, Size},
    ui::{RecordUi, Style, UiContext, UiInput, UiMemory},
};
use notwig_ext_devices::{LimiterDevice, TYPE_ID, release_id, threshold_id};

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
    assert!(texts.contains(&"Reset"));
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
