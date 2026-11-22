use std::collections::BTreeMap;
use std::sync::Arc;

use crate::error::ExtensionError;
use crate::web::compiled::{WebLoadedExtension, WebLoadedExtensions};
use crate::web::device::WebExtensionDeviceFactory;
use crate::web::instance::WebExtensionInstance;
use crate::web::package::WebExtensionPackage;
use crate::web::panel::WebExtensionPanelHost;
use notwig_core::dsp::{DeviceFactory, DeviceSpec, DeviceUiSpec};
use notwig_ext_api::{DevicePanelHost, ExtensionConfig, ExtensionInfo, ExtensionStatus};

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
        let mut infos = Vec::new();
        let mut errors = Vec::new();
        for package in packages {
            if !config.is_enabled(package.id()) {
                infos.push(disabled_info(&package));
                continue;
            }
            match load_package(&package) {
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
                }
                Err(error) => {
                    errors.push(format!("{}: {error}", package.id()));
                    infos.push(failed_info(&package, error));
                }
            }
        }
        Self {
            loaded: Arc::new(WebLoadedExtensions { by_type, infos }),
            errors,
        }
    }

    #[must_use]
    pub fn infos(&self) -> &[ExtensionInfo] {
        &self.loaded.infos
    }

    /// Per-package failures collected while loading, in package order.
    #[must_use]
    pub fn errors(&self) -> &[String] {
        &self.errors
    }

    #[must_use]
    pub fn specs(&self) -> Vec<DeviceSpec> {
        self.loaded.specs()
    }

    #[must_use]
    pub fn has_type(&self, type_id: &str) -> bool {
        self.loaded.by_type.contains_key(type_id)
    }

    #[must_use]
    pub fn factory(&self) -> Arc<dyn DeviceFactory> {
        Arc::new(WebExtensionDeviceFactory::new(Arc::clone(&self.loaded)))
    }

    #[must_use]
    pub fn panel_host(&self) -> Box<dyn DevicePanelHost> {
        Box::new(WebExtensionPanelHost::new(Arc::clone(&self.loaded)))
    }
}

fn load_package(package: &WebExtensionPackage) -> Result<WebLoadedExtension, ExtensionError> {
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
        error: None,
    };
    Ok(WebLoadedExtension {
        manifest: package.manifest.clone(),
        module,
        specs,
        info,
    })
}

fn disabled_info(package: &WebExtensionPackage) -> ExtensionInfo {
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
        error: None,
    }
}

fn failed_info(package: &WebExtensionPackage, error: ExtensionError) -> ExtensionInfo {
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
        error: Some(error.to_string()),
    }
}

fn js_error(value: wasm_bindgen::JsValue) -> ExtensionError {
    ExtensionError::Wasm(value.as_string().unwrap_or_else(|| format!("{value:?}")))
}
