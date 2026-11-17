use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::types::{PluginDescriptor, PluginError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanRecord {
    pub descriptor: PluginDescriptor,
    pub blacklisted: bool,
    pub failures: u32,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginCache {
    records: BTreeMap<String, ScanRecord>,
}

impl PluginCache {
    #[must_use]
    pub fn load(path: &Path) -> Result<Self, PluginError> {
        match std::fs::read_to_string(path) {
            Ok(contents) => serde_json::from_str(&contents)
                .map_err(|err| PluginError::Scan(format!("invalid plugin cache: {err}"))),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(err) => Err(PluginError::Io(err.to_string())),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), PluginError> {
        let contents = serde_json::to_string_pretty(self)
            .map_err(|err| PluginError::Scan(format!("serialize plugin cache: {err}")))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| PluginError::Io(err.to_string()))?;
        }
        std::fs::write(path, contents).map_err(|err| PluginError::Io(err.to_string()))
    }

    pub fn record(&mut self, descriptor: PluginDescriptor) {
        let key = descriptor.type_id.clone();
        self.records.insert(
            key,
            ScanRecord {
                descriptor,
                blacklisted: false,
                failures: 0,
                last_error: None,
            },
        );
    }

    pub fn record_failure(&mut self, type_id: &str, error: &str) {
        if let Some(record) = self.records.get_mut(type_id) {
            record.failures = record.failures.saturating_add(1);
            record.last_error = Some(error.to_owned());
            if record.failures >= 3 {
                record.blacklisted = true;
            }
        }
    }

    #[must_use]
    pub fn is_blacklisted(&self, type_id: &str) -> bool {
        self.records
            .get(type_id)
            .is_some_and(|record| record.blacklisted)
    }

    #[must_use]
    pub fn descriptors(&self) -> Vec<PluginDescriptor> {
        self.records
            .values()
            .filter(|record| !record.blacklisted)
            .map(|record| record.descriptor.clone())
            .collect()
    }

    #[must_use]
    pub fn records(&self) -> &BTreeMap<String, ScanRecord> {
        &self.records
    }

    pub fn clear_blacklist(&mut self) {
        for record in self.records.values_mut() {
            record.blacklisted = false;
            record.failures = 0;
        }
    }
}
