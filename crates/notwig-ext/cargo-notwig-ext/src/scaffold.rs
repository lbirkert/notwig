use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::error::{Error, Result};

const DEFAULT_ICON: &[u8] = include_bytes!("../assets/icon.png");

/// Creates a new extension project directory.
///
/// Fails if `path` already exists. `package_name` defaults to the directory
/// name and `extension_id` defaults to `com.example.<name>`.
pub fn new(
    path: PathBuf,
    package_name: Option<String>,
    extension_id: Option<String>,
) -> Result<()> {
    if path.exists() {
        return Err(Error::message(format!("{} already exists", path.display())));
    }
    create(&path, package_name, extension_id)
}

/// Adds extension files to an existing directory.
///
/// `package_name` defaults to the directory name and `extension_id` defaults to
/// `com.example.<name>`.
pub fn init(
    path: PathBuf,
    package_name: Option<String>,
    extension_id: Option<String>,
) -> Result<()> {
    create(&path, package_name, extension_id)
}

fn create(path: &Path, package_name: Option<String>, extension_id: Option<String>) -> Result<()> {
    fs::create_dir_all(path).map_err(|error| Error::io(path, error))?;
    let directory_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty() && *name != ".")
        .map(ToOwned::to_owned)
        .or_else(|| {
            std::env::current_dir().ok().and_then(|dir| {
                dir.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
        })
        .ok_or_else(|| Error::message("could not derive an extension name from the path"))?;

    let package_name = package_name.unwrap_or_else(|| sanitize_package_name(&directory_name));
    let slug = package_name.replace('_', "-");
    let extension_id = extension_id.unwrap_or_else(|| format!("com.example.{slug}"));
    let display_name = display_name(&directory_name);

    let files = [
        (PathBuf::from("Cargo.toml"), cargo_toml(&package_name)),
        (PathBuf::from("src/lib.rs"), library_rs()),
        (PathBuf::from("src/device.rs"), device_rs(&extension_id)),
        (
            PathBuf::from("extension.json"),
            extension_json(&extension_id, &display_name),
        ),
    ];
    for (relative, _) in &files {
        let target = path.join(relative);
        if target.exists() {
            return Err(Error::message(format!(
                "{} already exists; init does not overwrite files",
                target.display()
            )));
        }
    }
    let icon = path.join("icon.png");
    if icon.exists() {
        return Err(Error::message(format!(
            "{} already exists; init does not overwrite files",
            icon.display()
        )));
    }

    fs::create_dir_all(path.join("src")).map_err(|error| Error::io(path.join("src"), error))?;
    for (relative, contents) in files {
        let target = path.join(relative);
        fs::write(&target, contents).map_err(|error| Error::io(&target, error))?;
    }
    fs::write(&icon, DEFAULT_ICON).map_err(|error| Error::io(&icon, error))?;

    println!("created extension {extension_id} in {}", path.display());
    println!(
        "next: cd {} && cargo notwig-ext build --entrypoint wasm && cargo notwig-ext build --package wasm",
        path.display()
    );
    Ok(())
}

fn sanitize_package_name(name: &str) -> String {
    let mut out = String::new();
    for character in name.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
            out.push(character.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    let out = out.trim_matches(['-', '_']).to_owned();
    if out.is_empty() {
        "my-extension".into()
    } else if out.starts_with(|character: char| character.is_ascii_digit()) {
        format!("ext-{out}")
    } else {
        out
    }
}

fn display_name(name: &str) -> String {
    let mut out = String::new();
    for word in name.split(['-', '_']).filter(|word| !word.is_empty()) {
        if !out.is_empty() {
            out.push(' ');
        }
        let mut characters = word.chars();
        if let Some(first) = characters.next() {
            out.extend(first.to_uppercase());
            out.push_str(characters.as_str());
        }
    }
    if out.is_empty() {
        "My Extension".into()
    } else {
        out
    }
}

fn cargo_toml(package_name: &str) -> String {
    let crates = crates_root();
    CARGO_TEMPLATE
        .replace("__PACKAGE_NAME__", package_name)
        .replace("__CORE_PATH__", &toml_path(&crates.join("notwig-core")))
        .replace("__UI_PATH__", &toml_path(&crates.join("notwig-ui")))
        .replace(
            "__EXT_API_PATH__",
            &toml_path(&crates.join("notwig-ext/notwig-ext-api")),
        )
}

fn crates_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").clone()
}

fn toml_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn library_rs() -> String {
    LIB_TEMPLATE.into()
}

fn device_rs(extension_id: &str) -> String {
    DEVICE_TEMPLATE.replace("__EXTENSION_ID__", extension_id)
}

fn extension_json(extension_id: &str, display_name: &str) -> String {
    let manifest = json!({
        "manifest_version": notwig_ext_api::CURRENT_MANIFEST_VERSION,
        "id": extension_id,
        "name": display_name,
        "vendor": "Example",
        "version": "0.1.0",
        "api_version": notwig_ext_api::ABI_VERSION,
        "capabilities": ["device", "ui.panel"],
        "devices": [{
            "name": display_name,
            "type_id": extension_id,
        }],
        "description": "A new Notwig extension.",
        "image": "icon.png"
    });
    let mut text = serde_json::to_string_pretty(&manifest).unwrap_or_default();
    text.push('\n');
    text
}

const CARGO_TEMPLATE: &str = r#"[package]
name = "__PACKAGE_NAME__"
version = "0.1.0"
edition = "2024"
license = "MPL-2.0"

[lib]
crate-type = ["cdylib", "rlib"]

[workspace]

[dependencies]
notwig-core = { path = "__CORE_PATH__", default-features = false }
notwig-ui = { path = "__UI_PATH__", default-features = false }
notwig-ext-api = { path = "__EXT_API_PATH__" }
uuid = { version = "1", default-features = false, features = ["serde", "std"] }
"#;

const DEVICE_TEMPLATE: &str = r#"use egui::{Align2, FontId, Sense, Vec2};
use notwig_core::dsp::{DeviceSpec, DeviceUiSpec, ParamSpec, Unit};
use notwig_core::{DeviceKind, ParamId, SchemaVersion};
use notwig_ext_api::{DeviceExtension, DeviceMeterSnapshot, egui, panel_frame};
use notwig_ui::{knob, palette, section};
use uuid::Uuid;

pub const TYPE_ID: &str = "__EXTENSION_ID__";

const GAIN_UUID: u128 = 0x6761_696e_0000_0000_0000_0000_0000_0001;
const MIN_GAIN_DB: f64 = -24.0;
const MAX_GAIN_DB: f64 = 24.0;
const DEFAULT_GAIN_DB: f64 = 0.0;

#[must_use]
pub fn gain_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(GAIN_UUID))
}

fn gain_norm(db: f64) -> f32 {
    ((db - MIN_GAIN_DB) / (MAX_GAIN_DB - MIN_GAIN_DB)) as f32
}

#[derive(Debug, Clone)]
pub struct MyDevice {
    gain_db: f64,
    gain: f32,
}

impl Default for MyDevice {
    fn default() -> Self {
        Self {
            gain_db: DEFAULT_GAIN_DB,
            gain: 1.0,
        }
    }
}

impl MyDevice {
    fn update_gain(&mut self) {
        self.gain = 10.0_f32.powf((self.gain_db / 20.0) as f32);
    }
}

impl DeviceExtension for MyDevice {
    fn new() -> Self {
        Self::default()
    }

    fn spec() -> DeviceSpec {
        DeviceSpec {
            type_id: TYPE_ID.into(),
            version: SchemaVersion::V0,
            kind: DeviceKind::AudioFx,
            ports: Vec::new(),
            params: vec![ParamSpec::continuous(
                gain_id(),
                "Gain",
                Unit::Decibels,
                MIN_GAIN_DB,
                MAX_GAIN_DB,
                DEFAULT_GAIN_DB,
            )],
            latency_samples: 0,
            name: "My Device".into(),
            vendor: "Example".into(),
            ui: DeviceUiSpec::custom(format!("ext:{TYPE_ID}"), Some(260.0)),
            container: None,
        }
    }

    fn draw(&mut self, ui: &mut egui::Ui, _meter: DeviceMeterSnapshot) {
        section(ui, "My Device");
        let (rect, _) = ui.allocate_exact_size(Vec2::new(78.0, 58.0), Sense::hover());
        if let Some(norm) = knob(
            ui,
            ui.make_persistent_id("gain"),
            rect,
            gain_norm(self.gain_db),
            gain_norm(DEFAULT_GAIN_DB),
            true,
            "Gain",
        ) {
            self.gain_db = MIN_GAIN_DB + f64::from(norm) * (MAX_GAIN_DB - MIN_GAIN_DB);
            self.update_gain();
            panel_frame::set_param_normalized(ui, gain_id(), norm);
        }
        ui.painter().with_clip_rect(rect).text(
            egui::pos2(rect.center().x, rect.bottom() + 8.0),
            Align2::CENTER_TOP,
            format!("{:+.1} dB", self.gain_db),
            FontId::proportional(10.0),
            palette::TEXT_MUTED,
        );
    }

    fn set_param(&mut self, param_id: ParamId, value: f64) -> bool {
        if param_id != gain_id() {
            return false;
        }
        self.gain_db = value.clamp(MIN_GAIN_DB, MAX_GAIN_DB);
        self.update_gain();
        true
    }

    fn process(&mut self, samples: &mut [f32]) {
        for sample in samples {
            *sample *= self.gain;
        }
    }
}
"#;

const LIB_TEMPLATE: &str = r"//! Extension wiring and lifecycle entrypoints.

mod device;

pub use device::*;

use notwig_ext_api::{EP_NOTWIG_DEVICES, ExtensionBuilder, UnloadContext, notwig_ext};

#[notwig_ext(load)]
pub fn load(extension: &mut ExtensionBuilder) {
    extension.register_extension::<MyDevice>(EP_NOTWIG_DEVICES);
}

#[notwig_ext(unload)]
pub fn unload(_context: &mut UnloadContext) {}
";
