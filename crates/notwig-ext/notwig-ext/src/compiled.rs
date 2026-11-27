use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use notwig_core::dsp::DeviceSpec;
use notwig_ext_api::{
    AudioSubEditorDescriptor, CommandDescriptor, ExtensionInfo, ExtensionManifest,
    ExtensionPointDescriptor, ExtensionStatus,
};
use wasmtime::Module;

use crate::abi::{ExtensionEngines, ExtensionInstance as WasmInstance};
use crate::error::ExtensionError;
use crate::native::NativeLibrary;

pub(crate) enum ExtensionRuntime {
    Wasm(Module),
    Native(Arc<NativeLibrary>),
}

impl ExtensionRuntime {
    pub(crate) fn preload(
        &self,
        engines: &ExtensionEngines,
        extension_id: &str,
    ) -> Result<(), ExtensionError> {
        match self {
            Self::Wasm(module) => {
                let mut instance =
                    WasmInstance::instantiate_preload(engines.engine(), module, extension_id)?;
                instance.preload_phase()?;
                Ok(())
            }
            Self::Native(library) => library.preload(),
        }
    }

    pub(crate) fn load(
        &self,
        _engines: &ExtensionEngines,
        _extension_id: &str,
    ) -> Result<(), ExtensionError> {
        match self {
            Self::Wasm(_) => Ok(()),
            Self::Native(library) => library.load_phase(),
        }
    }
}

pub(crate) struct LoadedExtension {
    pub(crate) dir: PathBuf,
    pub(crate) manifest: ExtensionManifest,
    pub(crate) enabled: bool,
    pub(crate) runtime: Option<ExtensionRuntime>,
    pub(crate) specs: Vec<DeviceSpec>,
    pub(crate) commands: Vec<CommandDescriptor>,
    pub(crate) audio_sub_editors: Vec<AudioSubEditorDescriptor>,
    pub(crate) points: Vec<ExtensionPointDescriptor>,
    pub(crate) status: ExtensionStatus,
    pub(crate) error: Option<String>,
}

pub(crate) struct LoadedExtensions {
    pub(crate) engines: Arc<ExtensionEngines>,
    pub(crate) by_type: BTreeMap<String, Arc<LoadedExtension>>,
    pub(crate) by_command: BTreeMap<String, Arc<LoadedExtension>>,
    pub(crate) by_audio_sub_editor: BTreeMap<String, Arc<LoadedExtension>>,
    pub(crate) commands: BTreeMap<String, CommandDescriptor>,
    pub(crate) audio_sub_editors: BTreeMap<String, AudioSubEditorDescriptor>,
    pub(crate) points: BTreeMap<String, ExtensionPointDescriptor>,
    pub(crate) infos: Vec<ExtensionInfo>,
}

impl LoadedExtensions {
    pub(crate) fn find_type(&self, type_id: &str) -> Option<&Arc<LoadedExtension>> {
        self.by_type.get(type_id)
    }

    pub(crate) fn find_command(&self, command_id: &str) -> Option<&Arc<LoadedExtension>> {
        self.by_command.get(command_id)
    }

    pub(crate) fn find_audio_sub_editor(&self, editor_id: &str) -> Option<&Arc<LoadedExtension>> {
        self.by_audio_sub_editor.get(editor_id)
    }

    pub(crate) fn commands(&self) -> Vec<CommandDescriptor> {
        self.commands.values().cloned().collect()
    }

    pub(crate) fn audio_sub_editors(&self) -> Vec<AudioSubEditorDescriptor> {
        let mut editors: Vec<_> = self.audio_sub_editors.values().cloned().collect();
        editors.sort_by(|a, b| {
            a.category
                .cmp(&b.category)
                .then_with(|| a.order.cmp(&b.order))
                .then_with(|| a.title.cmp(&b.title))
                .then_with(|| a.id.cmp(&b.id))
        });
        editors
    }

    pub(crate) fn points(&self) -> Vec<ExtensionPointDescriptor> {
        self.points.values().cloned().collect()
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
