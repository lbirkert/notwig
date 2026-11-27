use std::collections::BTreeMap;
use std::sync::Arc;

use notwig_ext_api::{BottomPanelContext, BottomPanelFrame, BottomPanelHost, ExtensionCapability};

use crate::compiled::LoadedExtensions;
use crate::instance::ExtensionInstance;

struct UiInstance {
    instance: ExtensionInstance,
    handle: i32,
}

impl Drop for UiInstance {
    fn drop(&mut self) {
        self.instance.destroy_bottom_panel(self.handle);
    }
}

/// Host-side drawer for extension-rendered bottom panels.
pub struct ExtensionBottomPanelHost {
    loaded: Arc<LoadedExtensions>,
    instances: BTreeMap<String, UiInstance>,
}

impl ExtensionBottomPanelHost {
    pub(crate) fn new(loaded: Arc<LoadedExtensions>) -> Self {
        Self {
            loaded,
            instances: BTreeMap::new(),
        }
    }
}

impl BottomPanelHost for ExtensionBottomPanelHost {
    fn draw_bottom_panel(
        &mut self,
        panel_id: &str,
        context: &BottomPanelContext,
    ) -> Option<BottomPanelFrame> {
        let extension = self.loaded.find_bottom_panel(panel_id)?;
        if !extension
            .manifest
            .has_capability(ExtensionCapability::BottomPanel)
        {
            return None;
        }
        extension
            .bottom_panels
            .iter()
            .find(|panel| panel.id == panel_id)?;
        let runtime = extension.runtime.as_ref()?;
        if !self.instances.contains_key(panel_id) {
            let mut instance = ExtensionInstance::instantiate(
                runtime,
                &self.loaded.engines,
                &extension.manifest.id,
            )
            .map_err(|error| {
                tracing::warn!(
                    extension = %extension.manifest.id,
                    panel = panel_id,
                    error = %error,
                    "could not create extension bottom panel instance"
                );
                error
            })
            .ok()?;
            let handle = instance.create_bottom_panel(panel_id).ok()?;
            self.instances
                .insert(panel_id.to_owned(), UiInstance { instance, handle });
        }
        let result = {
            let ui = self.instances.get_mut(panel_id)?;
            ui.instance.draw_bottom_panel(ui.handle, context)
        };
        match result {
            Ok(frame) => Some(frame),
            Err(error) => {
                tracing::warn!(
                    extension = %extension.manifest.id,
                    panel = panel_id,
                    error = %error,
                    "extension bottom panel draw failed"
                );
                None
            }
        }
    }
}
