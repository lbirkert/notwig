use notwig_dsp::DeviceUiSpec;
use notwig_ext_api::{
    DeviceExtension, DeviceMeterSnapshot, DrawCommand, PanelCommand,
    geometry::{Point, Rect, Size},
    ui::{RecordUi, Style, UiContext, UiInput, UiMemory},
};
use notwig_ext_devices::{
    GATE_TYPE_ID, GateDevice, gate_attack_id, gate_hold_id, gate_range_id, gate_release_id,
    gate_threshold_id,
};

fn target() -> Rect {
    Rect::from_min_size(Point::new(0.0, 0.0), Size::new(260.0, 200.0))
}

#[test]
fn spec_is_a_valid_device_spec() {
    let spec = GateDevice::spec();
    assert_eq!(spec.type_id, GATE_TYPE_ID);
    assert_eq!(spec.name, "Gate");
    assert_eq!(spec.params.len(), 5);
    assert!(
        spec.params
            .iter()
            .any(|param| param.id == gate_threshold_id())
    );
    assert!(spec.params.iter().any(|param| param.id == gate_attack_id()));
    assert!(spec.params.iter().any(|param| param.id == gate_hold_id()));
    assert!(
        spec.params
            .iter()
            .any(|param| param.id == gate_release_id())
    );
    assert!(spec.params.iter().any(|param| param.id == gate_range_id()));
    assert!(matches!(spec.ui, DeviceUiSpec::Custom { .. }));
}

#[test]
fn draw_produces_a_valid_panel() {
    let mut gate = GateDevice::new();
    let input = UiInput::default();
    let mut memory = UiMemory::new();
    let panel = {
        let mut record = RecordUi::new(&input, &mut memory);
        {
            let mut ui = UiContext::new(&mut record, Style::default(), target());
            gate.draw(&mut ui);
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
    assert!(texts.iter().any(|text| text.eq_ignore_ascii_case("Gate")));
    for label in ["Thr", "Atk", "Hold", "Rel", "Rng"] {
        assert!(texts.contains(&label), "missing {label}: {texts:?}");
    }
}

#[test]
fn draw_uses_the_engine_meter_snapshot() {
    let mut gate = GateDevice::new();
    let input = UiInput::default();
    let mut memory = UiMemory::new();
    let meter = DeviceMeterSnapshot {
        input_peak_l: 1.0,
        input_peak_r: 1.0,
        output_peak_l: 0.0001,
        output_peak_r: 0.0001,
    };
    let panel = {
        let mut record = RecordUi::new(&input, &mut memory);
        {
            let mut ui = UiContext::new_with_meter(&mut record, Style::default(), target(), meter);
            gate.draw(&mut ui);
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
    assert!(texts.contains(&"CLOSED -80 dB"), "texts: {texts:?}");
}

#[test]
fn gate_opens_on_loud_signal_and_closes_on_quiet_signal() {
    let mut gate = GateDevice::new();
    gate.prepare(48_000, 2);
    gate.set_param(gate_threshold_id(), -40.0);
    gate.set_param(gate_attack_id(), 0.001);
    gate.set_param(gate_hold_id(), 0.0);
    gate.set_param(gate_release_id(), 0.01);
    gate.set_param(gate_range_id(), -80.0);

    let mut loud = vec![1.0_f32; 4096];
    gate.process(&mut loud);
    let open = *loud.last().unwrap();
    assert!(open > 0.95, "gate did not open: {open}");

    let mut quiet = vec![0.001_f32; 48_000];
    gate.process(&mut quiet);
    let closed = *quiet.last().unwrap();
    assert!(closed < 0.000_1, "gate did not close: {closed}");
}

#[test]
fn unknown_parameters_are_rejected() {
    let mut gate = GateDevice::new();
    assert!(!gate.set_param(notwig_core::ParamId::nil(), 1.0));
}
