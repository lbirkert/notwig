use std::collections::BTreeMap;
use std::sync::Arc;

use notwig_ext_api::{BottomPanelContext, BottomPanelFrame, BottomPanelHost, ExtensionCapability};

use crate::web::compiled::WebLoadedExtensions;
use crate::web::instance::WebExtensionInstance;

struct UiInstance {
    instance: WebExtensionInstance,
    handle: i32,
}

impl Drop for UiInstance {
    fn drop(&mut self) {
        self.instance.destroy_bottom_panel(self.handle);
    }
}

pub(crate) struct WebExtensionBottomPanelHost {
    loaded: Arc<WebLoadedExtensions>,
    instances: BTreeMap<String, UiInstance>,
}

impl WebExtensionBottomPanelHost {
    pub(crate) fn new(loaded: Arc<WebLoadedExtensions>) -> Self {
        Self {
            loaded,
            instances: BTreeMap::new(),
        }
    }
}

impl BottomPanelHost for WebExtensionBottomPanelHost {
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
        if !self.instances.contains_key(panel_id) {
            let mut instance =
                WebExtensionInstance::instantiate(&extension.module, &extension.manifest.id)
                    .map_err(|error| {
                        tracing::warn!(
                            extension = %extension.manifest.id,
                            panel = panel_id,
                            error = %error,
                            "could not create browser extension bottom panel instance"
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
                    "browser extension bottom panel draw failed"
                );
                None
            }
        }
    }
}
