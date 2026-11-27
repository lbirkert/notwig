use std::collections::BTreeMap;
use std::sync::Arc;

use notwig_core::dsp::DeviceSpec;
use notwig_ext_api::{
    AudioSubEditorDescriptor, CommandDescriptor, ExtensionInfo, ExtensionManifest,
};

pub(crate) struct WebLoadedExtension {
    pub(crate) manifest: ExtensionManifest,
    pub(crate) module: js_sys::WebAssembly::Module,
    pub(crate) specs: Vec<DeviceSpec>,
    pub(crate) commands: Vec<CommandDescriptor>,
    pub(crate) audio_sub_editors: Vec<AudioSubEditorDescriptor>,
    pub(crate) info: ExtensionInfo,
}

pub(crate) struct WebLoadedExtensions {
    pub(crate) by_type: BTreeMap<String, Arc<WebLoadedExtension>>,
    pub(crate) by_command: BTreeMap<String, Arc<WebLoadedExtension>>,
    pub(crate) by_audio_sub_editor: BTreeMap<String, Arc<WebLoadedExtension>>,
    pub(crate) commands: BTreeMap<String, CommandDescriptor>,
    pub(crate) audio_sub_editors: BTreeMap<String, AudioSubEditorDescriptor>,
    pub(crate) infos: Vec<ExtensionInfo>,
}

impl WebLoadedExtensions {
    pub(crate) fn find_type(&self, type_id: &str) -> Option<&Arc<WebLoadedExtension>> {
        self.by_type.get(type_id)
    }

    pub(crate) fn find_command(&self, command_id: &str) -> Option<&Arc<WebLoadedExtension>> {
        self.by_command.get(command_id)
    }

    pub(crate) fn find_audio_sub_editor(
        &self,
        editor_id: &str,
    ) -> Option<&Arc<WebLoadedExtension>> {
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

    pub(crate) fn specs(&self) -> Vec<DeviceSpec> {
        let mut seen = std::collections::BTreeSet::new();
        self.by_type
            .values()
            .flat_map(|extension| extension.specs.iter().cloned())
            .filter(|spec| seen.insert(spec.type_id.clone()))
            .collect()
    }
}
