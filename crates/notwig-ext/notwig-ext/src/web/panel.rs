use std::collections::BTreeMap;
use std::sync::Arc;

use notwig_ext_api::{DeviceDrawContext, DevicePanelHost, ExtensionCapability, PanelFrame};

use crate::web::compiled::WebLoadedExtensions;
use crate::web::instance::WebExtensionInstance;

struct UiInstance {
    instance: WebExtensionInstance,
    handle: i32,
}

pub(crate) struct WebExtensionPanelHost {
    loaded: Arc<WebLoadedExtensions>,
    instances: BTreeMap<String, UiInstance>,
}

impl WebExtensionPanelHost {
    pub(crate) fn new(loaded: Arc<WebLoadedExtensions>) -> Self {
        Self {
            loaded,
            instances: BTreeMap::new(),
        }
    }
}

impl DevicePanelHost for WebExtensionPanelHost {
    fn draw_device_panel(
        &mut self,
        type_id: &str,
        context: &DeviceDrawContext,
    ) -> Option<PanelFrame> {
        let extension = self.loaded.find_type(type_id)?;
        if !extension
            .manifest
            .has_capability(ExtensionCapability::UiPanel)
        {
            return None;
        }
        let device_key = context.device_id.as_str();
        if !self.instances.contains_key(device_key) {
            let mut instance =
                WebExtensionInstance::instantiate(&extension.module, &extension.manifest.id)
                    .map_err(|error| {
                        tracing::warn!(
                            extension = %extension.manifest.id,
                            type_id,
                            device_id = %context.device_id,
                            error = %error,
                            "could not create extension panel instance"
                        );
                        error
                    })
                    .ok()?;
            let handle = instance.create_device(type_id).ok()?;
            self.instances
                .insert(context.device_id.clone(), UiInstance { instance, handle });
        }
        let result = {
            let ui = self.instances.get_mut(device_key)?;
            for (param_id, value) in &context.params {
                let _ = ui.instance.set_param(ui.handle, *param_id, value);
            }
            ui.instance.draw_panel(ui.handle, context)
        };
        match result {
            Ok(frame) => Some(frame),
            Err(error) => {
                tracing::warn!(
                    extension = %extension.manifest.id,
                    type_id,
                    device_id = %context.device_id,
                    error = %error,
                    "extension panel draw failed"
                );
                // Keep the UI instance so a transient frame error does not
                // recreate the guest context and flash the panel.
                None
            }
        }
    }
}
