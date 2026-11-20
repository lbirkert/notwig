use notwig_plugins::{PluginDescriptor, PluginHost};
use notwig_ui::PluginEntry;

pub(crate) fn new_plugin_host() -> PluginHost {
    PluginHost::new()
}

pub(crate) struct PluginCatalog;

#[allow(clippy::unused_self)]
impl PluginCatalog {
    pub(crate) fn global() -> &'static Self {
        static CATALOG: std::sync::OnceLock<PluginCatalog> = std::sync::OnceLock::new();
        CATALOG.get_or_init(|| Self)
    }

    pub(crate) fn descriptors(&self) -> Vec<PluginDescriptor> {
        Vec::new()
    }

    pub(crate) fn poll(&self) -> bool {
        false
    }
}

pub(crate) fn plugin_entries(_descriptors: &[PluginDescriptor]) -> Vec<PluginEntry> {
    Vec::new()
}
