use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use crate::cache::PluginCache;
use crate::types::{
    PluginDescriptor, PluginError, PluginFormat, PluginInstance, PluginMetadata, PluginState,
};

/// Crash-isolating metadata probe.
///
/// Implementations are expected to load the plugin outside this process and
/// return only serializable metadata. The host process never calls plugin code
/// until the probe succeeds.
pub trait PluginProbe: Send + Sync {
    fn probe(&self, descriptor: &PluginDescriptor) -> Result<PluginMetadata, PluginError>;
}

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
    probe: Option<Arc<dyn PluginProbe>>,
    probed: BTreeMap<String, PluginMetadata>,
}

impl std::fmt::Debug for PluginHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginHost")
            .field("adapters", &self.adapters.len())
            .field("cached", &self.cache.descriptors().len())
            .field("probed", &self.probed.len())
            .field("probe", &self.probe.is_some())
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
        #[allow(unused_mut)]
        let mut host = Self::empty();
        #[cfg(feature = "vst3")]
        host.register_adapter(Box::new(crate::adapters::Vst3Adapter));
        #[cfg(feature = "vst2")]
        host.register_adapter(Box::new(crate::adapters::Vst2Adapter));
        #[cfg(all(feature = "au", target_vendor = "apple"))]
        host.register_adapter(Box::new(crate::adapters::AuAdapter));
        #[cfg(feature = "clap")]
        host.register_adapter(Box::new(crate::adapters::ClapAdapter));
        #[cfg(feature = "lv2")]
        host.register_adapter(Box::new(crate::adapters::Lv2Adapter));
        #[cfg(all(feature = "au3", target_vendor = "apple"))]
        host.register_adapter(Box::new(crate::adapters::Au3Adapter));
        host
    }

    #[must_use]
    pub fn empty() -> Self {
        Self {
            adapters: Vec::new(),
            cache: PluginCache::default(),
            probe: None,
            probed: BTreeMap::new(),
        }
    }

    /// Installs a crash-isolating metadata probe used before in-process loads.
    pub fn set_probe(&mut self, probe: Arc<dyn PluginProbe>) {
        self.probe = Some(probe);
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
            if let Ok(descriptors) = adapter.scan(paths) {
                for descriptor in descriptors {
                    if !self.cache.is_blacklisted(&descriptor.type_id) {
                        discovered.push(descriptor);
                    }
                }
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

    fn adapter_for(&self, format: PluginFormat) -> Result<&dyn HostAdapter, PluginError> {
        self.adapters
            .iter()
            .find(|adapter| adapter.format() == format)
            .map(std::convert::AsRef::as_ref)
            .ok_or(PluginError::BackendUnavailable(format))
    }

    /// Loads plugin metadata through the probe when one is installed.
    ///
    /// The probe runs before any plugin code executes in this process. Without
    /// a probe this falls back to an in-process load.
    pub fn probe_metadata(
        &mut self,
        descriptor: &PluginDescriptor,
    ) -> Result<PluginMetadata, PluginError> {
        if !descriptor.format.enabled() {
            return Err(PluginError::FormatDisabled(descriptor.format));
        }
        if self.cache.is_blacklisted(&descriptor.type_id) {
            return Err(PluginError::Blacklisted(descriptor.type_id.clone()));
        }
        if let Some(metadata) = self.probed.get(&descriptor.type_id) {
            return Ok(metadata.clone());
        }
        let metadata = if let Some(probe) = &self.probe {
            match probe.probe(descriptor) {
                Ok(metadata) => metadata,
                Err(err) => {
                    self.cache
                        .record_failure(&descriptor.type_id, &err.to_string());
                    return Err(err);
                }
            }
        } else {
            let instance = self.load_direct(descriptor)?;
            PluginMetadata {
                capabilities: instance.capabilities(),
                params: instance.describe_parameters(),
            }
        };
        self.probed
            .insert(descriptor.type_id.clone(), metadata.clone());
        Ok(metadata)
    }

    pub fn load(
        &mut self,
        descriptor: &PluginDescriptor,
    ) -> Result<Box<dyn PluginInstance>, PluginError> {
        if self.probe.is_some() {
            self.probe_metadata(descriptor)?;
        }
        self.load_direct(descriptor)
    }

    fn load_direct(
        &mut self,
        descriptor: &PluginDescriptor,
    ) -> Result<Box<dyn PluginInstance>, PluginError> {
        if !descriptor.format.enabled() {
            return Err(PluginError::FormatDisabled(descriptor.format));
        }
        if self.cache.is_blacklisted(&descriptor.type_id) {
            return Err(PluginError::Blacklisted(descriptor.type_id.clone()));
        }
        let result = self.adapter_for(descriptor.format)?.load(descriptor);
        if !self.cache.records().contains_key(&descriptor.type_id) {
            self.cache.record(descriptor.clone());
        }
        match result {
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
        self.adapter_for(descriptor.format)?.open_editor(descriptor)
    }
}
