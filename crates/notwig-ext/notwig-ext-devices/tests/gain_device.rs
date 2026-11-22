use notwig_ext_api::{
    DeviceExtension, DeviceMeterSnapshot, PanelInput, PanelModifiers, ThemeSnapshot, egui,
    panel_frame,
};
use notwig_ext_devices::{GAIN_TYPE_ID, GainDevice, gain_id};

#[test]
fn gain_device_draws_a_valid_frame() {
    let mut device = GainDevice::default();
    let context = egui::Context::default();
    let input = PanelInput {
        width: 320.0,
        height: 180.0,
        pixels_per_point: 1.0,
        time: Some(0.0),
        focused: true,
        max_texture_side: 4096,
        modifiers: PanelModifiers::default(),
        events: Vec::new(),
    };
    let frame = panel_frame::run_frame(&context, input, &ThemeSnapshot::default(), |ui| {
        device.draw(ui, DeviceMeterSnapshot::default());
    });
    frame.validate().unwrap();
    assert_ne!(frame.primitives.len(), 0);
    assert_ne!(frame.textures.len(), 0);
    let spec = GainDevice::spec();
    assert_eq!(spec.type_id, GAIN_TYPE_ID);
    let notwig_core::dsp::DeviceUiSpec::Custom { renderer, .. } = &spec.ui else {
        panic!("gain device must use the extension renderer");
    };
    assert_eq!(renderer, &format!("ext:{GAIN_TYPE_ID}"));
}

#[test]
fn gain_device_param_drives_gain() {
    let mut device = GainDevice::default();
    assert!(device.set_param(gain_id(), 6.0));
    let mut samples = [1.0_f32, 0.5];
    device.process(&mut samples);
    assert!((samples[0] - 1.995_262).abs() < 0.001);
    assert!((samples[1] - 0.997_631).abs() < 0.001);
}
