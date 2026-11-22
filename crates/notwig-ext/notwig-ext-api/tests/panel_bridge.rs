use notwig_core::ParamId;
use notwig_ext_api::{PanelAction, PanelInput, PanelModifiers, ThemeSnapshot, panel_frame};

#[test]
fn guest_frame_contains_meshes_textures_and_actions() {
    let context = egui::Context::default();
    let input = PanelInput {
        width: 320.0,
        height: 200.0,
        pixels_per_point: 1.0,
        time: Some(0.0),
        focused: true,
        max_texture_side: 4096,
        modifiers: PanelModifiers::default(),
        events: Vec::new(),
    };
    let param_id = ParamId::nil();
    let frame = panel_frame::run_frame(&context, input, &ThemeSnapshot::default(), |ui| {
        ui.heading("Panel");
        ui.label("hello");
        panel_frame::set_param_normalized(ui, param_id, 0.25);
    });
    frame.validate().unwrap();
    assert_ne!(frame.primitives.len(), 0);
    assert_ne!(frame.textures.len(), 0);
    assert!(frame.actions.iter().any(|action| matches!(
        action,
        PanelAction::SetParamNormalized { param_id: id, norm }
            if *id == param_id && (*norm - 0.25).abs() < f32::EPSILON
    )));
}

#[test]
fn second_frame_does_not_reupload_the_font_atlas() {
    let context = egui::Context::default();
    let input = PanelInput {
        width: 320.0,
        height: 200.0,
        pixels_per_point: 1.0,
        time: Some(0.0),
        focused: true,
        max_texture_side: 4096,
        modifiers: PanelModifiers::default(),
        events: Vec::new(),
    };
    let first = panel_frame::run_frame(&context, input.clone(), &ThemeSnapshot::default(), |ui| {
        ui.label("hello");
    });
    let second = panel_frame::run_frame(&context, input, &ThemeSnapshot::default(), |ui| {
        ui.label("hello");
    });
    assert_ne!(first.textures.len(), 0);
    assert_eq!(second.textures.len(), 0);
}
