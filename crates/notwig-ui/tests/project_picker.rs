use notwig_core::{DeviceChainId, Meta, ProjectDoc, SampleRate, TrackId};
use notwig_ui::{NotwigUi, ProjectPickerAction, ProjectPickerView, RuntimeView};

fn doc() -> ProjectDoc {
    ProjectDoc::new_template(
        Meta {
            name: "Picker".into(),
            created_utc: 0,
            modified_utc: 0,
            sample_rate: SampleRate::DEFAULT,
            tempo_bpm: 120.0,
            time_sig_num: 4,
            time_sig_den: 4,
        },
        TrackId::nil(),
        DeviceChainId::nil(),
    )
    .unwrap()
}

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
fn project_picker_shows_saved_projects() {
    let doc = doc();
    let mut ui = NotwigUi::default();
    let ctx = egui::Context::default();
    let runtime = RuntimeView {
        project_picker: Some(ProjectPickerView {
            epoch: 1,
            action: ProjectPickerAction::Open,
            projects: vec!["Demo".into(), "Sketch".into()],
            loading: false,
            error: None,
            suggested_name: String::new(),
        }),
        ..Default::default()
    };

    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            ..Default::default()
        },
        |ctx| {
            ui.show(ctx, &doc, &runtime);
        },
    );
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            ..Default::default()
        },
        |ctx| {
            ui.show(ctx, &doc, &runtime);
        },
    );

    let mut texts = Vec::new();
    for clipped in &output.shapes {
        collect_text(&clipped.shape, &mut texts);
    }
    assert!(texts.iter().any(|text| text == "Open Project"));
    assert!(texts.iter().any(|text| text == "Demo"));
    assert!(texts.iter().any(|text| text == "Sketch"));
}
