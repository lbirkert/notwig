use std::path::PathBuf;

use notwig_core::ids::{DeviceChain, Track};
use notwig_core::*;
use notwig_ext_api::{
    ExtensionCapability, ExtensionIndexDevice, ExtensionIndexEntry, ExtensionInfo, ExtensionStatus,
    TrustLevel,
};
use notwig_ui::{NotwigUi, RuntimeView, SettingsCategory, UiIntent};
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

fn doc() -> ProjectDoc {
    let meta = Meta {
        name: "Extensions".into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: SampleRate::DEFAULT,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    };
    ProjectDoc::new_template(meta, id::<Track>(1), id::<DeviceChain>(100)).unwrap()
}

fn runtime() -> RuntimeView {
    let mut runtime = RuntimeView {
        extension_indexes: vec!["https://example.com/extensions.json".into()],
        extension_index_loaded: true,
        ..RuntimeView::default()
    };
    runtime.extension_index = vec![ExtensionIndexEntry {
        id: "notwig.ext.devices".into(),
        name: "Notwig Stock Devices".into(),
        vendor: "Notwig".into(),
        version: "0.1.0".into(),
        api_version: 1,
        entrypoint: "notwig_ext_devices.wasm".into(),
        capabilities: vec![ExtensionCapability::Device, ExtensionCapability::UiDraw],
        devices: vec![ExtensionIndexDevice {
            name: "Notwig Stock Limiter".into(),
            type_id: "notwig.ext.devices.limiter".into(),
        }],
        description: Some("A simple peak limiter.".into()),
        image: Some("icon.png".into()),
        url: "https://example.com/notwig.ext.devices.zip".into(),
    }];
    runtime.extensions = vec![ExtensionInfo {
        id: "notwig.ext.devices".into(),
        name: "Notwig Stock Devices".into(),
        vendor: "Notwig".into(),
        version: "0.1.0".into(),
        description: Some("A simple peak limiter.".into()),
        image_path: Some(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../notwig-ext-devices/icon.png"),
        ),
        capabilities: vec![ExtensionCapability::Device],
        status: ExtensionStatus::Loaded,
        enabled: true,
        trust: TrustLevel::Trusted,
        device_count: 1,
        error: None,
    }];
    runtime
}

fn render(ui: &mut NotwigUi, doc: &ProjectDoc, runtime: &RuntimeView) {
    let ctx = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280.0, 800.0),
        )),
        ..Default::default()
    };
    let _ = ctx.run(input, |ctx| {
        let _ = ui.show(ctx, doc, runtime);
    });
}

#[test]
fn extension_explorer_renders_index_and_installed_entries() {
    let doc = doc();
    let runtime = runtime();
    let mut ui = NotwigUi::default();
    ui.state.selection.select_track(id::<Track>(1), false);
    ui.settings_open = true;
    ui.settings_category = SettingsCategory::Extensions;
    render(&mut ui, &doc, &runtime);
}

#[test]
fn dropping_a_zip_on_the_extensions_page_emits_install() {
    let doc = doc();
    let runtime = runtime();
    let mut ui = NotwigUi::default();
    ui.settings_open = true;
    ui.settings_category = SettingsCategory::Extensions;
    let ctx = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280.0, 800.0),
        )),
        dropped_files: vec![egui::DroppedFile {
            path: Some(PathBuf::from("/tmp/test-extension.zip")),
            ..Default::default()
        }],
        ..Default::default()
    };
    let mut intents = Vec::new();
    let _ = ctx.run(input, |ctx| {
        intents = ui.show(ctx, &doc, &runtime);
    });
    let expected = PathBuf::from("/tmp/test-extension.zip");
    assert!(intents.iter().any(|intent| matches!(
        intent,
        UiIntent::InstallExtensionZip { path } if path == &expected
    )));
}
