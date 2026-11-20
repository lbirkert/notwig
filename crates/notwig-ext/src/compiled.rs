use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use notwig_dsp::DeviceSpec;
use notwig_ext_api::{ExtensionInfo, ExtensionManifest, ExtensionStatus, TrustLevel};
use wasmtime::Module;

use crate::abi::ExtensionEngines;

pub(crate) struct LoadedExtension {
    pub(crate) dir: PathBuf,
    pub(crate) manifest: ExtensionManifest,
    pub(crate) trust: TrustLevel,
    pub(crate) enabled: bool,
    pub(crate) module: Option<Module>,
    pub(crate) specs: Vec<DeviceSpec>,
    pub(crate) status: ExtensionStatus,
    pub(crate) error: Option<String>,
}

pub(crate) struct LoadedExtensions {
    pub(crate) engines: Arc<ExtensionEngines>,
    pub(crate) by_type: BTreeMap<String, Arc<LoadedExtension>>,
    pub(crate) infos: Vec<ExtensionInfo>,
}

impl LoadedExtensions {
    pub(crate) fn find_type(&self, type_id: &str) -> Option<&Arc<LoadedExtension>> {
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
