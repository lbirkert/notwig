use notwig_core::ids::{DeviceChain, Track};
use notwig_core::*;
use notwig_ui::{NotwigUi, RuntimeView, SettingsCategory};
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

fn doc() -> ProjectDoc {
    let meta = Meta {
        name: "Settings layout".into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: SampleRate::DEFAULT,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    };
    ProjectDoc::new_template(meta, id::<Track>(1), id::<DeviceChain>(100)).unwrap()
}

fn render(
    ctx: &egui::Context,
    ui: &mut NotwigUi,
    doc: &ProjectDoc,
    runtime: &RuntimeView,
) -> egui::FullOutput {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280.0, 800.0),
        )),
        ..Default::default()
    };
    ctx.run(input, |ctx| {
        let _ = ui.show(ctx, doc, runtime);
    })
}

fn assert_shape_within(shape: &egui::Shape, area: egui::Rect) {
    match shape {
        egui::Shape::Vec(shapes) => {
            for shape in shapes {
                assert_shape_within(shape, area);
            }
        }
        egui::Shape::Rect(rect) => {
            assert!(
                area.expand(1.0).contains_rect(rect.rect),
                "settings rect escaped the window: {:?} outside {area:?}",
                rect.rect
            );
        }
        egui::Shape::Text(text) => {
            let rect = egui::Rect::from_min_size(text.pos, text.galley.size());
            assert!(
                area.expand(1.0).contains_rect(rect),
                "settings text escaped the window: {:?} outside {area:?}",
                text.galley.text()
            );
        }
        _ => {}
    }
}

#[test]
fn settings_content_stays_inside_the_window() {
    let doc = doc();
    let runtime = RuntimeView::default();
    let mut ui = NotwigUi::default();
    ui.settings_open = true;
    ui.settings_category = SettingsCategory::Audio;
    let ctx = egui::Context::default();
    let _ = render(&ctx, &mut ui, &doc, &runtime);
    let output = render(&ctx, &mut ui, &doc, &runtime);
    let area = ctx
        .memory(|memory| memory.area_rect(egui::Id::new("Settings")))
        .expect("settings window area");

    for clipped in &output.shapes {
        if !area.contains_rect(clipped.clip_rect) {
            continue;
        }
        assert_shape_within(&clipped.shape, area);
    }
}
