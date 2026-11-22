use std::collections::BTreeMap;
use std::sync::Arc;

use notwig_core::dsp::DeviceSpec;
use notwig_ext_api::{ExtensionInfo, ExtensionManifest};

pub(crate) struct WebLoadedExtension {
    pub(crate) manifest: ExtensionManifest,
    pub(crate) module: js_sys::WebAssembly::Module,
    pub(crate) specs: Vec<DeviceSpec>,
    pub(crate) info: ExtensionInfo,
}

pub(crate) struct WebLoadedExtensions {
    pub(crate) by_type: BTreeMap<String, Arc<WebLoadedExtension>>,
    pub(crate) infos: Vec<ExtensionInfo>,
}

impl WebLoadedExtensions {
    pub(crate) fn find_type(&self, type_id: &str) -> Option<&Arc<WebLoadedExtension>> {
        self.by_type.get(type_id)
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
