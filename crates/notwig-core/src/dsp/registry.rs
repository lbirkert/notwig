use std::collections::BTreeMap;

use crate::SchemaVersion;

use super::devices::DeviceSpec;

#[derive(Debug, Clone, Default)]
pub struct DeviceRegistry {
    specs: BTreeMap<String, Vec<DeviceSpec>>,
}

impl DeviceRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, spec: DeviceSpec) {
        let bucket = self.specs.entry(spec.type_id.clone()).or_default();
        if let Some(existing) = bucket.iter_mut().find(|s| s.version == spec.version) {
            *existing = spec;
        } else {
            bucket.push(spec);
        }
    }

    #[must_use]
    pub fn resolve(&self, type_id: &str, version: SchemaVersion) -> Option<&DeviceSpec> {
        self.specs
            .get(type_id)
            .and_then(|bucket| bucket.iter().find(|spec| spec.version == version))
    }

    #[must_use]
    pub fn resolve_latest(&self, type_id: &str) -> Option<&DeviceSpec> {
        self.specs.get(type_id).and_then(|bucket| bucket.last())
    }

    pub fn iter(&self) -> impl Iterator<Item = &DeviceSpec> {
        self.specs.values().flatten()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.specs.values().map(Vec::len).sum()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
