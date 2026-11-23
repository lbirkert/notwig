use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use notwig_core::dsp::DeviceFactory;
use notwig_core::dsp::{DeviceSpec, DeviceUiSpec};
use notwig_ext_api::{DevicePanelHost, ExtensionInfo, ExtensionSettings, ExtensionStatus, target};
use wasmtime::Module;

use crate::abi::ExtensionEngines;
use crate::compiled::{ExtensionRuntime, LoadedExtension, LoadedExtensions};
use crate::device::ExtensionDeviceFactory;
use crate::discovery::{self, DiscoveryFailure};
use crate::error::ExtensionError;
use crate::instance::ExtensionInstance;
use crate::native::NativeLibrary;
use crate::panel::ExtensionPanelHost;

/// Loaded extension registry and entry point for host integration.
///
/// Load with [`ExtensionHost::load`], then use [`factory`](Self::factory) to
/// create audio processors and [`panel_host`](Self::panel_host) to draw device
/// panels. Loaded extensions are shared behind an `Arc`.
pub struct ExtensionHost {
    loaded: Arc<LoadedExtensions>,
}

impl ExtensionHost {
    /// Discovers and loads extensions under `root` using persisted settings.
    ///
    /// Failures are recorded as [`ExtensionInfo`] entries with
    /// [`ExtensionStatus::Failed`] instead of aborting the load. Disabled
    /// extensions are reported without being instantiated.
    pub fn load(
        root: &Path,
        settings: &BTreeMap<String, ExtensionSettings>,
    ) -> Result<Self, ExtensionError> {
        let engines = Arc::new(ExtensionEngines::new()?);
        let report = discovery::discover(root)?;
        let mut by_type = BTreeMap::new();
        let mut infos = Vec::new();

        for failure in report.failures {
            infos.push(failure_info(failure, settings));
        }

        for discovered in report.extensions {
            let id = discovered.manifest.id.clone();
            let extension = if notwig_ext_api::config::is_enabled(settings, &id) {
                load_enabled(&engines, discovered)
            } else {
                LoadedExtension {
                    dir: discovered.dir,
                    manifest: discovered.manifest,
                    enabled: false,
                    runtime: None,
                    specs: Vec::new(),
                    status: ExtensionStatus::Disabled,
                    error: None,
                }
            };
            let extension = Arc::new(extension);
            infos.push(info_from(&extension, settings));
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

    /// Returns the status of every discovered extension, including failures.
    #[must_use]
    pub fn infos(&self) -> &[ExtensionInfo] {
        &self.loaded.infos
    }

    /// Returns deduplicated device specs from all loaded extensions.
    #[must_use]
    pub fn specs(&self) -> Vec<DeviceSpec> {
        self.loaded.specs()
    }

    /// Returns `true` when a loaded extension provides `type_id`.
    #[must_use]
    pub fn has_type(&self, type_id: &str) -> bool {
        self.loaded.by_type.contains_key(type_id)
    }

    /// Creates a dynamic device factory for the loaded extensions.
    #[must_use]
    pub fn factory(&self) -> Arc<dyn DeviceFactory> {
        Arc::new(ExtensionDeviceFactory::new(Arc::clone(&self.loaded)))
    }

    /// Creates a panel host that keeps per-device UI state.
    #[must_use]
    pub fn panel_host(&self) -> Box<dyn DevicePanelHost> {
        Box::new(ExtensionPanelHost::new(Arc::clone(&self.loaded)))
    }
}

fn load_enabled(
    engines: &Arc<ExtensionEngines>,
    discovered: discovery::DiscoveredExtension,
) -> LoadedExtension {
    let discovery::DiscoveredExtension {
        manifest,
        dir,
        entry_path,
        target,
    } = discovered;
    let runtime = match load_runtime(engines, &entry_path, &target) {
        Ok(runtime) => runtime,
        Err(error) => return failed(manifest, dir, error),
    };
    let mut instance = match ExtensionInstance::instantiate(&runtime, engines, &manifest.id) {
        Ok(instance) => instance,
        Err(error) => return failed(manifest, dir, error.to_string()),
    };
    let mut specs = Vec::new();
    let declarations = manifest.devices.clone();
    for declaration in &declarations {
        let mut spec = match instance.describe(&declaration.type_id) {
            Ok(spec) => spec,
            Err(error) => return failed(manifest, dir, error.to_string()),
        };
        if spec.type_id != declaration.type_id {
            return failed(
                manifest,
                dir,
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
        enabled: true,
        runtime: Some(runtime),
        specs,
        status: ExtensionStatus::Loaded,
        error: None,
    }
}

fn load_runtime(
    engines: &ExtensionEngines,
    entry_path: &Path,
    target: &str,
) -> Result<ExtensionRuntime, ExtensionError> {
    if target::is_wasm(target) {
        let wasm = std::fs::read(entry_path).map_err(|error| {
            ExtensionError::Io(format!("read {}: {error}", entry_path.display()))
        })?;
        if !wasm.starts_with(b"\0asm") {
            return Err(ExtensionError::Manifest(
                "entrypoint is not a wasm module".into(),
            ));
        }
        let engine = engines.engine();
        let module = Module::from_binary(engine, &wasm)
            .map_err(|error| ExtensionError::Wasm(format!("compile wasm: {error}")))?;
        Ok(ExtensionRuntime::Wasm(module))
    } else {
        NativeLibrary::load(entry_path).map(ExtensionRuntime::Native)
    }
}

fn failed(
    manifest: notwig_ext_api::ExtensionManifest,
    dir: std::path::PathBuf,
    error: impl ToString,
) -> LoadedExtension {
    LoadedExtension {
        dir,
        manifest,
        enabled: true,
        runtime: None,
        specs: Vec::new(),
        status: ExtensionStatus::Failed,
        error: Some(error.to_string()),
    }
}

fn info_from(
    extension: &LoadedExtension,
    settings: &BTreeMap<String, ExtensionSettings>,
) -> ExtensionInfo {
    let image_path = extension.manifest.image.as_ref().and_then(|image| {
        let path = extension.dir.join(image);
        path.is_file().then_some(path)
    });
    let installed_hash = settings
        .get(&extension.manifest.id)
        .and_then(|settings| settings.installed_hash.clone());
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
        device_count: if extension.specs.is_empty() {
            extension.manifest.devices.len()
        } else {
            extension.specs.len()
        },
        targets: extension
            .manifest
            .entrypoints
            .iter()
            .map(|entry| entry.target.clone())
            .collect(),
        installed_hash,
        error: extension.error.clone(),
    }
}

fn failure_info(
    failure: DiscoveryFailure,
    settings: &BTreeMap<String, ExtensionSettings>,
) -> ExtensionInfo {
    let installed_hash = failure
        .id
        .as_ref()
        .and_then(|id| settings.get(id))
        .and_then(|settings| settings.installed_hash.clone());
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
        device_count: 0,
        targets: Vec::new(),
        installed_hash,
        error: Some(failure.error),
    }
}
