use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use notwig_dsp::DeviceFactory;
use notwig_dsp::{DeviceSpec, DeviceUiSpec};
use notwig_ext_api::{
    DevicePanelHost, ExtensionInfo, ExtensionSettings, ExtensionStatus, TrustLevel,
};
use wasmtime::Module;

use crate::abi::{ExtensionEngines, ExtensionInstance};
use crate::compiled::{LoadedExtension, LoadedExtensions};
use crate::device::ExtensionDeviceFactory;
use crate::discovery::{self, DiscoveryFailure};
use crate::error::ExtensionError;
use crate::panel::ExtensionPanelHost;

pub struct ExtensionHost {
    loaded: Arc<LoadedExtensions>,
}

impl ExtensionHost {
    pub fn load(
        root: &Path,
        settings: &BTreeMap<String, ExtensionSettings>,
    ) -> Result<Self, ExtensionError> {
        let engines = Arc::new(ExtensionEngines::new()?);
        let report = discovery::discover(root)?;
        let mut by_type = BTreeMap::new();
        let mut infos = Vec::new();

        for failure in report.failures {
            infos.push(failure_info(failure));
        }

        for discovered in report.extensions {
            let id = discovered.manifest.id.clone();
            let settings = settings.get(&id).copied().unwrap_or_default();
            let extension = if settings.enabled {
                load_enabled(&engines, discovered, settings.trust)
            } else {
                LoadedExtension {
                    dir: discovered.dir,
                    manifest: discovered.manifest,
                    trust: settings.trust,
                    enabled: false,
                    module: None,
                    specs: Vec::new(),
                    status: ExtensionStatus::Disabled,
                    error: None,
                }
            };
            let extension = Arc::new(extension);
            infos.push(info_from(&extension));
            if extension.status == ExtensionStatus::Loaded {
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
        }

        Ok(Self {
            loaded: Arc::new(LoadedExtensions {
                engines,
                by_type,
                infos,
            }),
        })
    }

    #[must_use]
    pub fn infos(&self) -> &[ExtensionInfo] {
        &self.loaded.infos
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
        Arc::new(ExtensionDeviceFactory::new(Arc::clone(&self.loaded)))
    }

    #[must_use]
    pub fn panel_host(&self) -> Box<dyn DevicePanelHost> {
        Box::new(ExtensionPanelHost::new(Arc::clone(&self.loaded)))
    }
}

fn load_enabled(
    engines: &Arc<ExtensionEngines>,
    discovered: discovery::DiscoveredExtension,
    trust: TrustLevel,
) -> LoadedExtension {
    let dir = discovered.dir;
    let manifest = discovered.manifest;
    let engine = engines.engine(trust);
    let module = match Module::from_binary(engine, &discovered.wasm) {
        Ok(module) => module,
        Err(error) => {
            return failed(manifest, dir, trust, format!("compile wasm: {error}"));
        }
    };
    let mut instance = match ExtensionInstance::instantiate(engine, &module, &manifest.id, trust) {
        Ok(instance) => instance,
        Err(error) => {
            return failed(manifest, dir, trust, error.to_string());
        }
    };
    let mut specs = Vec::new();
    let declarations = manifest.devices.clone();
    for declaration in &declarations {
        let mut spec = match instance.describe(&declaration.type_id) {
            Ok(spec) => spec,
            Err(error) => {
                return failed(manifest, dir, trust, error.to_string());
            }
        };
        if spec.type_id != declaration.type_id {
            return failed(
                manifest,
                dir,
                trust,
                format!(
                    "device type mismatch: declared {}, described {}",
                    declaration.type_id, spec.type_id
                ),
            );
        }
        if matches!(spec.ui, DeviceUiSpec::Generic) {
            spec.ui = DeviceUiSpec::custom(format!("ext:{}", spec.type_id), None);
        }
        specs.push(spec);
    }
    LoadedExtension {
        dir,
        manifest,
        trust,
        enabled: true,
        module: Some(module),
        specs,
        status: ExtensionStatus::Loaded,
        error: None,
    }
}

fn failed(
    manifest: notwig_ext_api::ExtensionManifest,
    dir: std::path::PathBuf,
    trust: TrustLevel,
    error: String,
) -> LoadedExtension {
    LoadedExtension {
        dir,
        manifest,
        trust,
        enabled: true,
        module: None,
        specs: Vec::new(),
        status: ExtensionStatus::Failed,
        error: Some(error),
    }
}

fn info_from(extension: &LoadedExtension) -> ExtensionInfo {
    let image_path = extension.manifest.image.as_ref().and_then(|image| {
        let path = extension.dir.join(image);
        path.is_file().then_some(path)
    });
    ExtensionInfo {
        id: extension.manifest.id.clone(),
        name: extension.manifest.name.clone(),
        vendor: extension.manifest.vendor.clone(),
        version: extension.manifest.version.clone(),
        description: extension.manifest.description.clone(),
        image_path,
        capabilities: extension.manifest.capabilities.clone(),
        status: extension.status,
        enabled: extension.enabled,
        trust: extension.trust,
        device_count: if extension.specs.is_empty() {
            extension.manifest.devices.len()
        } else {
            extension.specs.len()
        },
        error: extension.error.clone(),
    }
}

fn failure_info(failure: DiscoveryFailure) -> ExtensionInfo {
    ExtensionInfo {
        id: failure.id.unwrap_or_else(|| failure.name.clone()),
        name: failure.name,
        vendor: String::new(),
        version: String::new(),
        description: None,
        image_path: None,
        capabilities: Vec::new(),
        status: ExtensionStatus::Failed,
        enabled: true,
        trust: TrustLevel::Trusted,
        device_count: 0,
        error: Some(failure.error),
    }
}
