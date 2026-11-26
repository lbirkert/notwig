use notwig_ui::{NotwigUi, palette};

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

fn count_meshes(shape: &egui::Shape) -> usize {
    match shape {
        egui::Shape::Mesh(_) => 1,
        egui::Shape::Vec(shapes) => shapes.iter().map(count_meshes).sum(),
        _ => 0,
    }
}

fn run_splash(opacity: f32) -> egui::FullOutput {
    let ui = NotwigUi::default();
    let ctx = egui::Context::default();
    ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(1280.0, 900.0),
            )),
            ..Default::default()
        },
        |ctx| {
            ui.show_startup_splash(ctx, opacity);
        },
    )
}

#[test]
fn startup_splash_paints_the_wordmark_and_logo() {
    let output = run_splash(1.0);
    let mut texts = Vec::new();
    let mut meshes = 0;
    for clipped in &output.shapes {
        collect_text(&clipped.shape, &mut texts);
        meshes += count_meshes(&clipped.shape);
    }
    assert!(
        texts.iter().any(|text| text == "NOTWIG"),
        "the startup splash should paint the big wordmark"
    );
    assert!(
        meshes > 0,
        "the startup splash should paint the embedded logo"
    );
}

#[test]
fn splash_background_stays_opaque_while_text_fades() {
    let output = run_splash(0.1);
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1280.0, 900.0));
    let background = output.shapes.iter().find_map(|clipped| {
        if let egui::Shape::Rect(shape) = &clipped.shape
            && shape.rect == screen
        {
            Some(shape.fill)
        } else {
            None
        }
    });
    let background = background.expect("splash should paint its full-screen backdrop");
    assert_eq!(background, palette::APP_BAR);
    assert_eq!(background.a(), 255);

    let mut texts = Vec::new();
    for clipped in &output.shapes {
        collect_text(&clipped.shape, &mut texts);
    }
    assert_ne!(texts, Vec::<String>::new());
}

#[test]
fn zero_opacity_paints_only_the_opaque_backdrop() {
    let output = run_splash(0.0);
    let mut texts = Vec::new();
    let mut meshes = 0;
    let mut has_backdrop = false;
    for clipped in &output.shapes {
        collect_text(&clipped.shape, &mut texts);
        meshes += count_meshes(&clipped.shape);
        if let egui::Shape::Rect(shape) = &clipped.shape
            && shape.fill == palette::APP_BAR
        {
            has_backdrop = true;
        }
    }
    assert!(has_backdrop, "the splash backdrop should not fade in");
    assert!(
        texts.is_empty(),
        "text should be fully transparent at 0 opacity"
    );
    assert_eq!(meshes, 0, "logo should be fully transparent at 0 opacity");
}
