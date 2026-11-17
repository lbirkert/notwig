use std::path::Path;

use crate::cache::PluginCache;
use crate::types::{PluginDescriptor, PluginError, PluginFormat, PluginInstance, PluginState};

pub trait HostAdapter: Send + Sync {
    fn format(&self) -> PluginFormat;

    fn scan(&self, paths: &[std::path::PathBuf]) -> Result<Vec<PluginDescriptor>, PluginError>;

    fn load(&self, descriptor: &PluginDescriptor) -> Result<Box<dyn PluginInstance>, PluginError>;

    fn open_editor(&self, _descriptor: &PluginDescriptor) -> Result<(), PluginError> {
        Err(PluginError::BackendUnavailable(self.format()))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct UnavailableAdapter {
    format: PluginFormat,
}

impl UnavailableAdapter {
    #[must_use]
    pub fn new(format: PluginFormat) -> Self {
        Self { format }
    }
}

impl HostAdapter for UnavailableAdapter {
    fn format(&self) -> PluginFormat {
        self.format
    }

    fn scan(&self, _paths: &[std::path::PathBuf]) -> Result<Vec<PluginDescriptor>, PluginError> {
        Ok(Vec::new())
    }

    fn load(&self, _descriptor: &PluginDescriptor) -> Result<Box<dyn PluginInstance>, PluginError> {
        Err(PluginError::BackendUnavailable(self.format))
    }
}

pub struct PluginHost {
    adapters: Vec<Box<dyn HostAdapter>>,
    cache: PluginCache,
}

impl std::fmt::Debug for PluginHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginHost")
            .field("adapters", &self.adapters.len())
            .field("cached", &self.cache.descriptors().len())
            .finish()
    }
}

impl Default for PluginHost {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginHost {
    #[must_use]
    pub fn new() -> Self {
        let mut host = Self {
            adapters: Vec::new(),
            cache: PluginCache::default(),
        };
        for format in [PluginFormat::Vst3, PluginFormat::Au, PluginFormat::Vst2] {
            if format.enabled() {
                host.adapters
                    .push(Box::new(UnavailableAdapter::new(format)));
            }
        }
        host
    }

    pub fn register_adapter(&mut self, adapter: Box<dyn HostAdapter>) {
        self.adapters
            .retain(|existing| existing.format() != adapter.format());
        self.adapters.push(adapter);
    }

    pub fn load_cache(&mut self, path: &Path) -> Result<(), PluginError> {
        self.cache = PluginCache::load(path)?;
        Ok(())
    }

    pub fn save_cache(&self, path: &Path) -> Result<(), PluginError> {
        self.cache.save(path)
    }

    #[must_use]
    pub fn cache(&self) -> &PluginCache {
        &self.cache
    }

    pub fn scan(&mut self, paths: &[std::path::PathBuf]) -> Vec<PluginDescriptor> {
        let mut discovered = Vec::new();
        for adapter in &self.adapters {
            match adapter.scan(paths) {
                Ok(descriptors) => {
                    for descriptor in descriptors {
                        if !self.cache.is_blacklisted(&descriptor.type_id) {
                            discovered.push(descriptor);
                        }
                    }
                }
                Err(_) => {}
            }
        }
        for descriptor in &discovered {
            self.cache.record(descriptor.clone());
        }
        discovered.sort_by(|a, b| {
            a.format
                .cmp(&b.format)
                .then_with(|| a.path.cmp(&b.path))
                .then_with(|| a.uid.cmp(&b.uid))
        });
        discovered
    }

    pub fn load(
        &mut self,
        descriptor: &PluginDescriptor,
    ) -> Result<Box<dyn PluginInstance>, PluginError> {
        if !descriptor.format.enabled() {
            return Err(PluginError::FormatDisabled(descriptor.format));
        }
        if self.cache.is_blacklisted(&descriptor.type_id) {
            return Err(PluginError::Blacklisted(descriptor.type_id.clone()));
        }
        let Some(adapter) = self
            .adapters
            .iter()
            .find(|adapter| adapter.format() == descriptor.format)
        else {
            return Err(PluginError::BackendUnavailable(descriptor.format));
        };
        if !self.cache.records().contains_key(&descriptor.type_id) {
            self.cache.record(descriptor.clone());
        }
        match adapter.load(descriptor) {
            Ok(instance) => Ok(instance),
            Err(err) => {
                self.cache
                    .record_failure(&descriptor.type_id, &err.to_string());
                Err(err)
            }
        }
    }

    pub fn save_instance_state(
        &mut self,
        instance: &mut dyn PluginInstance,
    ) -> Result<PluginState, PluginError> {
        instance.save_state()
    }

    pub fn restore_instance_state(
        &mut self,
        instance: &mut dyn PluginInstance,
        state: &PluginState,
    ) -> Result<(), PluginError> {
        instance.load_state(state)
    }

    pub fn open_editor(&self, descriptor: &PluginDescriptor) -> Result<(), PluginError> {
        let Some(adapter) = self
            .adapters
            .iter()
            .find(|adapter| adapter.format() == descriptor.format)
        else {
            return Err(PluginError::BackendUnavailable(descriptor.format));
        };
        adapter.open_editor(descriptor)
    }
}
