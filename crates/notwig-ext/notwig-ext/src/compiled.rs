use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use notwig_core::dsp::DeviceSpec;
use notwig_ext_api::{CommandDescriptor, ExtensionInfo, ExtensionManifest, ExtensionStatus};
use wasmtime::Module;

use crate::abi::ExtensionEngines;
use crate::native::NativeLibrary;

pub(crate) enum ExtensionRuntime {
    Wasm(Module),
    Native(Arc<NativeLibrary>),
}

pub(crate) struct LoadedExtension {
    pub(crate) dir: PathBuf,
    pub(crate) manifest: ExtensionManifest,
    pub(crate) enabled: bool,
    pub(crate) runtime: Option<ExtensionRuntime>,
    pub(crate) specs: Vec<DeviceSpec>,
    pub(crate) commands: Vec<CommandDescriptor>,
    pub(crate) status: ExtensionStatus,
    pub(crate) error: Option<String>,
}

pub(crate) struct LoadedExtensions {
    pub(crate) engines: Arc<ExtensionEngines>,
    pub(crate) by_type: BTreeMap<String, Arc<LoadedExtension>>,
    pub(crate) by_command: BTreeMap<String, Arc<LoadedExtension>>,
    pub(crate) commands: BTreeMap<String, CommandDescriptor>,
    pub(crate) infos: Vec<ExtensionInfo>,
}

impl LoadedExtensions {
    pub(crate) fn find_type(&self, type_id: &str) -> Option<&Arc<LoadedExtension>> {
        self.by_type.get(type_id)
    }

    pub(crate) fn find_command(&self, command_id: &str) -> Option<&Arc<LoadedExtension>> {
        self.by_command.get(command_id)
    }

    pub(crate) fn commands(&self) -> Vec<CommandDescriptor> {
        self.commands.values().cloned().collect()
    }

    pub(crate) fn specs(&self) -> Vec<DeviceSpec> {
        let mut seen = std::collections::BTreeSet::new();
        self.by_type
            .values()
            .flat_map(|extension| extension.specs.iter().cloned())
            .filter(|spec| seen.insert(spec.type_id.clone()))
            .collect()
    }
}
