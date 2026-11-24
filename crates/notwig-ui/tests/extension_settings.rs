use std::path::PathBuf;
use std::sync::Arc;

use notwig_core::ids::{DeviceChain, Track};
use notwig_core::*;
use notwig_ext_api::{
    ExtensionCapability, ExtensionIndexDevice, ExtensionIndexEntry, ExtensionIndexUrl,
    ExtensionInfo, ExtensionStatus,
};
use notwig_ui::test_api::settings as settings_test;
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
        api_version: 5,
        urls: vec![ExtensionIndexUrl {
            url: "https://example.com/notwig.ext.devices.zip".into(),
            hash: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
            size: 2_700_000,
            targets: vec!["wasm".into()],
        }],
        capabilities: vec![ExtensionCapability::Device, ExtensionCapability::UiPanel],
        devices: vec![ExtensionIndexDevice {
            name: "Notwig Stock Limiter".into(),
            type_id: "notwig.ext.devices.limiter".into(),
        }],
        description: Some("A simple peak limiter.".into()),
        image: Some("icon.png".into()),
    }];
    runtime.extensions = vec![ExtensionInfo {
        id: "notwig.ext.devices".into(),
        name: "Notwig Stock Devices".into(),
        vendor: "Notwig".into(),
        version: "0.1.0".into(),
        description: Some("A simple peak limiter.".into()),
        image_path: Some(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../notwig-ext/notwig-ext-devices/icon.png"),
        ),
        capabilities: vec![ExtensionCapability::Device],
        status: ExtensionStatus::Loaded,
        enabled: true,
        device_count: 1,
        targets: vec!["wasm".into(), "macos-aarch64".into()],
        installed_hash: None,
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
fn extension_index_cards_render_hosted_icons() {
    let doc = doc();
    let mut runtime = runtime();
    runtime.extensions.clear();
    let icon = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../notwig-ext/notwig-ext-devices/icon.png"),
    )
    .unwrap();
    runtime
        .extension_icons
        .insert("notwig.ext.devices".into(), Arc::new(icon));
    let mut ui = NotwigUi::default();
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

fn installed_info(targets: &[&str], installed_hash: Option<&str>) -> ExtensionInfo {
    ExtensionInfo {
        id: "notwig.ext.devices".into(),
        name: "Notwig Stock Devices".into(),
        vendor: "Notwig".into(),
        version: "0.1.0".into(),
        description: None,
        image_path: None,
        capabilities: vec![ExtensionCapability::Device],
        status: ExtensionStatus::Loaded,
        enabled: true,
        device_count: 1,
        targets: targets.iter().map(|target| (*target).to_owned()).collect(),
        installed_hash: installed_hash.map(ToOwned::to_owned),
        error: None,
    }
}

fn index_entry(hash: &str, targets: &[&str]) -> ExtensionIndexEntry {
    ExtensionIndexEntry {
        id: "notwig.ext.devices".into(),
        name: "Notwig Stock Devices".into(),
        vendor: "Notwig".into(),
        version: "0.1.0".into(),
        api_version: 5,
        urls: vec![ExtensionIndexUrl {
            url: "https://example.com/notwig.ext.devices.zip".into(),
            hash: hash.into(),
            size: 123_456,
            targets: targets.iter().map(|target| (*target).to_owned()).collect(),
        }],
        capabilities: vec![ExtensionCapability::Device, ExtensionCapability::UiPanel],
        devices: Vec::new(),
        description: None,
        image: None,
    }
}

#[test]
fn target_tags_group_platforms_and_deduplicate_architectures() {
    let info = installed_info(
        &["wasm", "macos-aarch64", "macos-x86_64", "windows-x86_64"],
        None,
    );
    assert_eq!(
        settings_test::extension_target_labels(Some(&info), None),
        vec!["WASM", "macOS", "Windows"]
    );
}

#[test]
fn unsupported_targets_follow_the_host_target() {
    let info = installed_info(&["windows-x86_64"], None);
    assert_eq!(
        settings_test::extension_target_supported(Some(&info), None),
        notwig_ext_api::target::is_supported("windows-x86_64")
    );
    let wasm = installed_info(&["wasm"], None);
    assert!(settings_test::extension_target_supported(Some(&wasm), None));
}

#[test]
fn update_is_available_only_when_the_index_hash_changes() {
    let installed = installed_info(&["wasm"], Some("abc123"));
    assert!(settings_test::extension_update_available(
        Some(&installed),
        Some(&index_entry("def456", &["wasm"]))
    ));
    assert!(!settings_test::extension_update_available(
        Some(&installed),
        Some(&index_entry("ABC123", &["wasm"]))
    ));
    let unknown = installed_info(&["wasm"], None);
    assert!(!settings_test::extension_update_available(
        Some(&unknown),
        Some(&index_entry("def456", &["wasm"]))
    ));
}
