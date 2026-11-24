use notwig_ext_api::{
    DeviceMeterSnapshot, PanelFrame, PanelInput, PanelModifiers, ThemeSnapshot, egui, panel_frame,
};

pub fn draw_frame(
    width: f32,
    height: f32,
    mut draw: impl FnMut(&mut egui::Ui, DeviceMeterSnapshot),
) -> PanelFrame {
    let context = egui::Context::default();
    let input = PanelInput {
        width,
        height,
        pixels_per_point: 1.0,
        time: Some(0.0),
        focused: true,
        max_texture_side: 4096,
        modifiers: PanelModifiers::default(),
        events: Vec::new(),
    };
    panel_frame::run_frame(&context, input, &ThemeSnapshot::default(), |ui| {
        draw(ui, DeviceMeterSnapshot::default());
    })
}
