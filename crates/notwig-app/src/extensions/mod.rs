#[cfg(any(target_arch = "wasm32", test))]
use std::collections::BTreeSet;

mod index;
#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use native::ExtensionManager;
#[cfg(target_arch = "wasm32")]
pub(crate) use web::ExtensionManager;

#[allow(dead_code)]
pub(crate) enum ExtensionEvent {
    Loaded,
    Installed { name: String, version: String },
    Error(String),
}

/// IDs explicitly uninstalled in this browser profile. The marker prevents
/// bundled or otherwise automatic packages from reappearing after removal.
#[cfg(any(target_arch = "wasm32", test))]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct UninstalledExtensions(BTreeSet<String>);

#[cfg(any(target_arch = "wasm32", test))]
impl UninstalledExtensions {
    pub(crate) fn from_json(text: &str) -> Result<Self, String> {
        serde_json::from_str::<BTreeSet<String>>(text)
            .map(Self)
            .map_err(|error| format!("decode uninstalled extensions: {error}"))
    }

    pub(crate) fn to_json(&self) -> Result<String, String> {
        serde_json::to_string(&self.0)
            .map_err(|error| format!("encode uninstalled extensions: {error}"))
    }

    pub(crate) fn contains(&self, id: &str) -> bool {
        self.0.contains(id)
    }

    /// Marks `id` as uninstalled. Returns whether the state changed.
    pub(crate) fn mark(&mut self, id: &str) -> bool {
        self.0.insert(id.to_owned())
    }

    /// Clears the marker so a reinstall can be discovered. Returns whether the
    /// state changed.
    pub(crate) fn clear(&mut self, id: &str) -> bool {
        self.0.remove(id)
    }
}

#[cfg(test)]
mod tests {
    use super::UninstalledExtensions;

    #[test]
    fn uninstall_markers_hide_and_reappear_on_reinstall() {
        let mut uninstalled = UninstalledExtensions::default();
        assert!(!uninstalled.contains("notwig.ext.devices"));

        assert!(uninstalled.mark("notwig.ext.devices"));
        assert!(uninstalled.contains("notwig.ext.devices"));
        assert!(!uninstalled.mark("notwig.ext.devices"));

        let restored = UninstalledExtensions::from_json(&uninstalled.to_json().unwrap()).unwrap();
        assert!(restored.contains("notwig.ext.devices"));

        assert!(uninstalled.clear("notwig.ext.devices"));
        assert!(!uninstalled.contains("notwig.ext.devices"));
        assert!(!uninstalled.clear("notwig.ext.devices"));
    }
}
