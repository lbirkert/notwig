#![cfg(feature = "egui")]

use notwig_ext_api::{DeviceExtension, EguiInput, EguiModifiers, ThemeSnapshot, egui, egui_panel};
use notwig_ext_devices::{EGUI_GAIN_TYPE_ID, EguiGainDevice, egui_gain_id};

#[test]
fn egui_device_draws_a_valid_frame() {
    let mut device = EguiGainDevice::default();
    let context = egui::Context::default();
    let input = EguiInput {
        width: 320.0,
        height: 180.0,
        pixels_per_point: 1.0,
        time: Some(0.0),
        focused: true,
        max_texture_side: 4096,
        modifiers: EguiModifiers::default(),
        events: Vec::new(),
    };
    let frame = egui_panel::run_frame(&context, input, &ThemeSnapshot::default(), |ui| {
        device.draw_egui(ui);
    });
    frame.validate().unwrap();
    assert_ne!(frame.primitives.len(), 0);
    assert_ne!(frame.textures.len(), 0);
    assert_eq!(EguiGainDevice::spec().type_id, EGUI_GAIN_TYPE_ID);
}

#[test]
fn egui_device_param_drives_gain() {
    let mut device = EguiGainDevice::default();
    assert!(device.set_param(egui_gain_id(), 6.0));
    let mut samples = [1.0_f32, 0.5];
    device.process(&mut samples);
    assert!((samples[0] - 1.995_262).abs() < 0.001);
    assert!((samples[1] - 0.997_631).abs() < 0.001);
}
