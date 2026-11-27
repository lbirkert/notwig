use std::collections::BTreeMap;
use std::sync::Arc;

use crate::error::ExtensionError;
use crate::web::compiled::{WebLoadedExtension, WebLoadedExtensions};
use crate::web::device::WebExtensionDeviceFactory;
use crate::web::instance::WebExtensionInstance;
use crate::web::package::WebExtensionPackage;
use crate::web::panel::WebExtensionPanelHost;
use crate::web::sub_editor::WebExtensionAudioSubEditorHost;
use notwig_core::dsp::{DeviceFactory, DeviceSpec, DeviceUiSpec};
use notwig_ext_api::{
    AudioSubEditorDescriptor, AudioSubEditorHost, CommandContext, CommandDescriptor, CommandEffect,
    DevicePanelHost, ExtensionConfig, ExtensionInfo, ExtensionStatus,
};

/// Browser host for WASM extension packages.
///
/// Loads packages with the platform WebAssembly API and mirrors the native
/// host's device factory and panel interfaces.
pub struct WebExtensionHost {
    loaded: Arc<WebLoadedExtensions>,
    errors: Vec<String>,
}

impl WebExtensionHost {
    /// Loads browser extension packages. The browser build supports WASM
    /// entrypoints only, and disabled packages are reported without being
    /// instantiated.
    #[must_use]
    pub fn load(packages: Vec<WebExtensionPackage>, config: &ExtensionConfig) -> Self {
        let mut by_type = BTreeMap::new();
        let mut by_command = BTreeMap::new();
        let mut commands = BTreeMap::new();
        let mut audio_sub_editors = BTreeMap::new();
        let mut by_audio_sub_editor = BTreeMap::new();
        let mut infos = Vec::new();
        let mut errors = Vec::new();
        for package in packages {
            let installed_hash = config
                .settings
                .get(package.id())
                .and_then(|settings| settings.installed_hash.clone());
            if !config.is_enabled(package.id()) {
                infos.push(disabled_info(&package, installed_hash));
                continue;
            }
            match load_package(&package, installed_hash) {
                Ok(extension) => {
                    let extension = Arc::new(extension);
                    infos.push(extension.info.clone());
                    for spec in &extension.specs {
                        if by_type.contains_key(&spec.type_id) {
                            tracing::warn!(
                                type_id = %spec.type_id,
                                extension = %extension.manifest.id,
                                "duplicate extension device type id"
                            );
                        } else {
                            by_type.insert(spec.type_id.clone(), Arc::clone(&extension));
                        }
                    }
                    for command in &extension.commands {
                        if commands.contains_key(&command.id) {
                            tracing::warn!(
                                command = %command.id,
                                extension = %extension.manifest.id,
                                "duplicate browser extension command id"
                            );
                        } else {
                            commands.insert(command.id.clone(), command.clone());
                            by_command.insert(command.id.clone(), Arc::clone(&extension));
                        }
                    }
                    for editor in &extension.audio_sub_editors {
                        if audio_sub_editors.contains_key(&editor.id) {
                            tracing::warn!(
                                sub_editor = %editor.id,
                                extension = %extension.manifest.id,
                                "duplicate browser extension audio sub-editor id"
                            );
                        } else {
                            audio_sub_editors.insert(editor.id.clone(), editor.clone());
                            by_audio_sub_editor.insert(editor.id.clone(), Arc::clone(&extension));
                        }
                    }
                }
                Err(error) => {
                    errors.push(format!("{}: {error}", package.id()));
                    infos.push(failed_info(&package, error, config));
                }
            }
        }
        Self {
            loaded: Arc::new(WebLoadedExtensions {
                by_type,
                by_command,
                by_audio_sub_editor,
                commands,
                audio_sub_editors,
                infos,
            }),
            errors,
        }
    }

    /// Returns the status of every supplied package.
    #[must_use]
    pub fn infos(&self) -> &[ExtensionInfo] {
        &self.loaded.infos
    }

    /// Per-package failures collected while loading, in package order.
    #[must_use]
    pub fn errors(&self) -> &[String] {
        &self.errors
    }

    /// Returns deduplicated device specs from loaded packages.
    #[must_use]
    pub fn specs(&self) -> Vec<DeviceSpec> {
        self.loaded.specs()
    }

    /// Returns resolved command descriptors in deterministic id order.
    #[must_use]
    pub fn commands(&self) -> Vec<CommandDescriptor> {
        self.loaded.commands()
    }

    /// Returns resolved audio-editor sub-editors from all loaded packages.
    #[must_use]
    pub fn audio_sub_editors(&self) -> Vec<AudioSubEditorDescriptor> {
        self.loaded.audio_sub_editors()
    }

    /// Executes a contributed command against a bounded context.
    pub fn execute_command(
        &self,
        id: &str,
        context: &CommandContext,
    ) -> Result<CommandEffect, ExtensionError> {
        let extension = self
            .loaded
            .find_command(id)
            .ok_or_else(|| ExtensionError::UnknownCommand(id.to_owned()))?;
        let mut instance =
            WebExtensionInstance::instantiate(&extension.module, &extension.manifest.id)?;
        instance.execute_command(id, context)
    }

    /// Returns `true` when a loaded package provides `type_id`.
    #[must_use]
    pub fn has_type(&self, type_id: &str) -> bool {
        self.loaded.by_type.contains_key(type_id)
    }

    /// Creates a dynamic device factory for the loaded packages.
    #[must_use]
    pub fn factory(&self) -> Arc<dyn DeviceFactory> {
        Arc::new(WebExtensionDeviceFactory::new(Arc::clone(&self.loaded)))
    }

    /// Creates a panel host that keeps per-device UI state.
    #[must_use]
    pub fn panel_host(&self) -> Box<dyn DevicePanelHost> {
        Box::new(WebExtensionPanelHost::new(Arc::clone(&self.loaded)))
    }

    /// Creates a host that keeps per-clip extension sub-editor UI state.
    #[must_use]
    pub fn audio_sub_editor_host(&self) -> Box<dyn AudioSubEditorHost> {
        Box::new(WebExtensionAudioSubEditorHost::new(Arc::clone(
            &self.loaded,
        )))
    }
}

fn load_package(
    package: &WebExtensionPackage,
    installed_hash: Option<String>,
) -> Result<WebLoadedExtension, ExtensionError> {
    let wasm = js_sys::Uint8Array::from(package.wasm.as_slice());
    let module = js_sys::WebAssembly::Module::new(&wasm).map_err(js_error)?;
    let mut instance = WebExtensionInstance::instantiate(&module, &package.manifest.id)?;
    let mut specs = Vec::new();
    for declaration in &package.manifest.devices {
        let mut spec = instance.describe(&declaration.type_id)?;
        if spec.type_id != declaration.type_id {
            return Err(ExtensionError::Manifest(format!(
                "device type mismatch: declared {}, described {}",
                declaration.type_id, spec.type_id
            )));
        }
        if matches!(spec.ui, DeviceUiSpec::Generic) {
            spec.ui = DeviceUiSpec::custom(format!("ext:{}", spec.type_id), None);
        }
        specs.push(spec);
    }
    let info = ExtensionInfo {
        id: package.manifest.id.clone(),
        name: package.manifest.name.clone(),
        vendor: package.manifest.vendor.clone(),
        version: package.manifest.version.clone(),
        description: package.manifest.description.clone(),
        image_path: None,
        capabilities: package.manifest.capabilities.clone(),
        status: ExtensionStatus::Loaded,
        enabled: true,
        device_count: specs.len(),
        targets: package
            .manifest
            .entrypoints
            .iter()
            .map(|entry| entry.target.clone())
            .collect(),
        installed_hash,
        error: None,
    };
    let commands = package
        .manifest
        .command_contributions()
        .map(|contribution| CommandDescriptor {
            id: contribution.id.clone(),
            title: contribution.title.clone().unwrap_or_default(),
            category: contribution.category.clone().unwrap_or_default(),
            description: contribution.description.clone().unwrap_or_default(),
            when: contribution.when.clone(),
            default_bindings: contribution.default_bindings.clone(),
            extension_id: Some(package.manifest.id.clone()),
        })
        .collect();
    let audio_sub_editors = package
        .manifest
        .audio_sub_editor_contributions()
        .map(|contribution| AudioSubEditorDescriptor {
            id: contribution.id.clone(),
            title: contribution.title.clone().unwrap_or_default(),
            category: contribution.category.clone().unwrap_or_default(),
            order: contribution.order,
            extension_id: Some(package.manifest.id.clone()),
        })
        .collect();
    Ok(WebLoadedExtension {
        manifest: package.manifest.clone(),
        module,
        specs,
        commands,
        audio_sub_editors,
        info,
    })
}

fn disabled_info(package: &WebExtensionPackage, installed_hash: Option<String>) -> ExtensionInfo {
    ExtensionInfo {
        id: package.manifest.id.clone(),
        name: package.manifest.name.clone(),
        vendor: package.manifest.vendor.clone(),
        version: package.manifest.version.clone(),
        description: package.manifest.description.clone(),
        image_path: None,
        capabilities: package.manifest.capabilities.clone(),
        status: ExtensionStatus::Disabled,
        enabled: false,
        device_count: package.manifest.devices.len(),
        targets: package
            .manifest
            .entrypoints
            .iter()
            .map(|entry| entry.target.clone())
            .collect(),
        installed_hash,
        error: None,
    }
}

fn failed_info(
    package: &WebExtensionPackage,
    error: ExtensionError,
    config: &ExtensionConfig,
) -> ExtensionInfo {
    ExtensionInfo {
        id: package.manifest.id.clone(),
        name: package.manifest.name.clone(),
        vendor: package.manifest.vendor.clone(),
        version: package.manifest.version.clone(),
        description: package.manifest.description.clone(),
        image_path: None,
        capabilities: package.manifest.capabilities.clone(),
        status: ExtensionStatus::Failed,
        enabled: true,
        device_count: package.manifest.devices.len(),
        targets: package
            .manifest
            .entrypoints
            .iter()
            .map(|entry| entry.target.clone())
            .collect(),
        installed_hash: config
            .settings
            .get(package.id())
            .and_then(|settings| settings.installed_hash.clone()),
        error: Some(error.to_string()),
    }
}

fn js_error(value: wasm_bindgen::JsValue) -> ExtensionError {
    ExtensionError::Wasm(value.as_string().unwrap_or_else(|| format!("{value:?}")))
}
