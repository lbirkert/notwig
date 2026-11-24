use notwig_ui::NotwigUi;

fn collect_text(shape: &egui::Shape, texts: &mut Vec<String>) {
    match shape {
        egui::Shape::Text(text) => texts.push(text.galley.text().to_owned()),
        egui::Shape::Vec(shapes) => {
            for shape in shapes {
                collect_text(shape, texts);
            }
        }
        _ => {}
    }
}

#[test]
fn startup_splash_paints_the_wordmark() {
    let ui = NotwigUi::default();
    let ctx = egui::Context::default();
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(1280.0, 900.0),
            )),
            ..Default::default()
        },
        |ctx| {
            ui.show_startup_splash(ctx, 1.0);
        },
    );
    let mut texts = Vec::new();
    for clipped in &output.shapes {
        collect_text(&clipped.shape, &mut texts);
    }
    assert!(
        texts.iter().any(|text| text == "NOTWIG"),
        "the startup splash should paint the big wordmark"
    );
}

#[test]
fn fully_faded_splash_paints_nothing() {
    let ui = NotwigUi::default();
    let ctx = egui::Context::default();
    let output = ctx.run(egui::RawInput::default(), |ctx| {
        ui.show_startup_splash(ctx, 0.0);
    });
    assert_eq!(output.shapes.len(), 0);
}
