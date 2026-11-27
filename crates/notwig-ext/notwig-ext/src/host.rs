use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use notwig_core::dsp::DeviceFactory;
use notwig_core::dsp::{DeviceSpec, DeviceUiSpec};
use notwig_ext_api::{
    AudioSubEditorDescriptor, AudioSubEditorHost, CommandContext, CommandDescriptor, CommandEffect,
    DevicePanelHost, ExtensionInfo, ExtensionManifest, ExtensionPointDescriptor, ExtensionSettings,
    ExtensionStatus, target,
};
use wasmtime::Module;

use crate::abi::ExtensionEngines;
use crate::compiled::{ExtensionRuntime, LoadedExtension, LoadedExtensions};
use crate::contributions;
use crate::device::ExtensionDeviceFactory;
use crate::discovery::{self, DiscoveryFailure};
use crate::error::ExtensionError;
use crate::instance::ExtensionInstance;
use crate::native::NativeLibrary;
use crate::panel::ExtensionPanelHost;
use crate::sub_editor::ExtensionAudioSubEditorHost;

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
        let mut by_command = BTreeMap::new();
        let mut commands = BTreeMap::new();
        let mut audio_sub_editors = BTreeMap::new();
        let mut by_audio_sub_editor = BTreeMap::new();
        let mut points = BTreeMap::new();
        let mut infos = Vec::new();

        for failure in report.failures {
            infos.push(failure_info(failure, settings));
        }

        let mut pending = Vec::new();
        for discovered in report.extensions {
            let id = discovered.manifest.id.clone();
            if notwig_ext_api::config::is_enabled(settings, &id) {
                match prepare_enabled(&engines, discovered) {
                    Ok(prepared) => pending.push(Some(PendingExtension::Prepared(prepared))),
                    Err(failed) => pending.push(Some(PendingExtension::Loaded(*failed))),
                }
            } else {
                pending.push(Some(PendingExtension::Loaded(LoadedExtension {
                    dir: discovered.dir,
                    manifest: discovered.manifest,
                    enabled: false,
                    runtime: None,
                    specs: Vec::new(),
                    commands: Vec::new(),
                    audio_sub_editors: Vec::new(),
                    points: Vec::new(),
                    status: ExtensionStatus::Disabled,
                    error: None,
                })));
            }
        }

        // Preload every enabled extension before any load-phase registration
        // runs. A candidate that fails preload can fall back to the next target.
        for slot in &mut pending {
            let Some(item) = slot.take() else {
                continue;
            };
            *slot = Some(match item {
                PendingExtension::Loaded(loaded) => PendingExtension::Loaded(loaded),
                PendingExtension::Prepared(prepared) => {
                    match preload_prepared(&engines, prepared) {
                        Ok(prepared) => PendingExtension::Prepared(prepared),
                        Err(failed) => PendingExtension::Loaded(*failed),
                    }
                }
            });
        }

        let mut extensions = Vec::with_capacity(pending.len());
        for slot in pending {
            let Some(item) = slot else {
                continue;
            };
            extensions.push(match item {
                PendingExtension::Loaded(loaded) => loaded,
                PendingExtension::Prepared(prepared) => activate_prepared(&engines, prepared),
            });
        }

        for extension in extensions {
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
                for command in &extension.commands {
                    if commands.contains_key(&command.id) {
                        tracing::warn!(
                            command = %command.id,
                            extension = %extension.manifest.id,
                            "duplicate extension command id"
                        );
                    } else {
                        commands.insert(command.id.clone(), command.clone());
                        by_command.insert(command.id.clone(), Arc::clone(&extension));
                    }
                }
                for point in &extension.points {
                    if points.contains_key(&point.id) {
                        tracing::warn!(
                            point = %point.id,
                            extension = %extension.manifest.id,
                            "duplicate extension point id"
                        );
                    } else {
                        points.insert(point.id.clone(), point.clone());
                    }
                }
                for editor in &extension.audio_sub_editors {
                    if audio_sub_editors.contains_key(&editor.id) {
                        tracing::warn!(
                            sub_editor = %editor.id,
                            extension = %extension.manifest.id,
                            "duplicate extension audio sub-editor id"
                        );
                    } else {
                        audio_sub_editors.insert(editor.id.clone(), editor.clone());
                        by_audio_sub_editor.insert(editor.id.clone(), Arc::clone(&extension));
                    }
                }
            }
        }

        Ok(Self {
            loaded: Arc::new(LoadedExtensions {
                engines,
                by_type,
                by_command,
                by_audio_sub_editor,
                commands,
                audio_sub_editors,
                points,
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

    /// Returns resolved command descriptors in deterministic id order.
    #[must_use]
    pub fn commands(&self) -> Vec<CommandDescriptor> {
        self.loaded.commands()
    }

    /// Returns resolved audio-editor sub-editors from all loaded extensions.
    #[must_use]
    pub fn audio_sub_editors(&self) -> Vec<AudioSubEditorDescriptor> {
        self.loaded.audio_sub_editors()
    }

    /// Returns extension points declared by loaded extensions.
    #[must_use]
    pub fn extension_points(&self) -> Vec<ExtensionPointDescriptor> {
        self.loaded.points()
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
        let runtime = extension
            .runtime
            .as_ref()
            .ok_or_else(|| ExtensionError::Disabled(extension.manifest.id.clone()))?;
        let mut instance =
            ExtensionInstance::instantiate(runtime, &self.loaded.engines, &extension.manifest.id)?;
        instance.execute_command(id, context)
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

    /// Creates a host that keeps per-clip extension sub-editor UI state.
    #[must_use]
    pub fn audio_sub_editor_host(&self) -> Box<dyn AudioSubEditorHost> {
        Box::new(ExtensionAudioSubEditorHost::new(Arc::clone(&self.loaded)))
    }
}

struct PreparedExtension {
    dir: PathBuf,
    manifest: ExtensionManifest,
    runtime: ExtensionRuntime,
    remaining: VecDeque<(String, PathBuf)>,
}

enum PendingExtension {
    Loaded(LoadedExtension),
    Prepared(PreparedExtension),
}

fn prepare_enabled(
    engines: &Arc<ExtensionEngines>,
    discovered: discovery::DiscoveredExtension,
) -> Result<PreparedExtension, Box<LoadedExtension>> {
    let discovery::DiscoveredExtension {
        manifest,
        dir,
        candidates,
        ..
    } = discovered;
    let mut candidates: VecDeque<_> = candidates.into();
    let mut last_error = "no supported entrypoint".to_owned();
    while let Some((target, entry_path)) = candidates.pop_front() {
        match load_runtime(engines, &entry_path, &target) {
            Ok(runtime) => {
                return Ok(PreparedExtension {
                    dir,
                    manifest,
                    runtime,
                    remaining: candidates,
                });
            }
            Err(error) => {
                tracing::warn!(
                    extension = %manifest.id,
                    target = %target,
                    error = %error,
                    "extension entrypoint failed to load; trying the next target"
                );
                last_error = error.to_string();
            }
        }
    }
    Err(Box::new(failed(manifest, dir, last_error)))
}

fn preload_prepared(
    engines: &Arc<ExtensionEngines>,
    mut prepared: PreparedExtension,
) -> Result<PreparedExtension, Box<LoadedExtension>> {
    loop {
        match prepared.runtime.preload(engines, &prepared.manifest.id) {
            Ok(()) => return Ok(prepared),
            Err(error) => {
                tracing::warn!(
                    extension = %prepared.manifest.id,
                    error = %error,
                    "extension preload failed; trying the next target"
                );
                if !advance_runtime(engines, &mut prepared) {
                    return Err(Box::new(failed(
                        prepared.manifest,
                        prepared.dir,
                        error.to_string(),
                    )));
                }
            }
        }
    }
}

fn activate_prepared(
    engines: &Arc<ExtensionEngines>,
    mut prepared: PreparedExtension,
) -> LoadedExtension {
    let mut preloaded = true;
    loop {
        if !preloaded {
            if let Err(error) = prepared.runtime.preload(engines, &prepared.manifest.id) {
                tracing::warn!(
                    extension = %prepared.manifest.id,
                    error = %error,
                    "fallback extension preload failed; trying the next target"
                );
                if !advance_runtime(engines, &mut prepared) {
                    return failed(prepared.manifest, prepared.dir, error.to_string());
                }
                continue;
            }
        }
        match activate_candidate(engines, &prepared.runtime, &prepared.manifest) {
            Ok((specs, resolved)) => {
                return LoadedExtension {
                    dir: prepared.dir,
                    manifest: prepared.manifest,
                    enabled: true,
                    runtime: Some(prepared.runtime),
                    specs,
                    commands: resolved.commands,
                    audio_sub_editors: resolved.audio_sub_editors,
                    points: resolved.points,
                    status: ExtensionStatus::Loaded,
                    error: None,
                };
            }
            Err(error) => {
                tracing::warn!(
                    extension = %prepared.manifest.id,
                    error = %error,
                    "extension activation failed; trying the next target"
                );
                if !advance_runtime(engines, &mut prepared) {
                    return failed(prepared.manifest, prepared.dir, error);
                }
                preloaded = false;
            }
        }
    }
}

fn activate_candidate(
    engines: &Arc<ExtensionEngines>,
    runtime: &ExtensionRuntime,
    manifest: &ExtensionManifest,
) -> Result<(Vec<DeviceSpec>, contributions::ResolvedContributions), String> {
    runtime
        .load(engines, &manifest.id)
        .map_err(|error| error.to_string())?;
    let mut instance = ExtensionInstance::instantiate(runtime, engines, &manifest.id)
        .map_err(|error| error.to_string())?;
    let mut specs = Vec::new();
    for declaration in &manifest.devices {
        let mut spec = instance
            .describe(&declaration.type_id)
            .map_err(|error| format!("describe device {}: {error}", declaration.type_id))?;
        if spec.type_id != declaration.type_id {
            return Err(format!(
                "device type mismatch: declared {}, described {}",
                declaration.type_id, spec.type_id
            ));
        }
        if matches!(spec.ui, DeviceUiSpec::Generic) {
            spec.ui = DeviceUiSpec::custom(format!("ext:{}", spec.type_id), None);
        }
        specs.push(spec);
    }
    let registrations = instance
        .registrations()
        .map_err(|error| error.to_string())?;
    let resolved =
        contributions::resolve(manifest, registrations).map_err(|error| error.to_string())?;
    Ok((specs, resolved))
}

fn advance_runtime(engines: &Arc<ExtensionEngines>, prepared: &mut PreparedExtension) -> bool {
    let mut last_error = None;
    while let Some((target, entry_path)) = prepared.remaining.pop_front() {
        match load_runtime(engines, &entry_path, &target) {
            Ok(runtime) => {
                prepared.runtime = runtime;
                return true;
            }
            Err(error) => {
                tracing::warn!(
                    extension = %prepared.manifest.id,
                    target = %target,
                    error = %error,
                    "extension entrypoint failed to load; trying the next target"
                );
                last_error = Some(error.to_string());
            }
        }
    }
    if let Some(error) = last_error {
        tracing::warn!(extension = %prepared.manifest.id, %error, "no remaining entrypoint");
    }
    false
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
        commands: Vec::new(),
        audio_sub_editors: Vec::new(),
        points: Vec::new(),
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
