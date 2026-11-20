use std::collections::BTreeMap;
use std::sync::Arc;

use notwig_ext_api::{DeviceDrawContext, DevicePanelHost, PanelDraw};

use crate::abi::ExtensionInstance;
use crate::compiled::LoadedExtensions;

struct UiInstance {
    instance: ExtensionInstance,
    handle: i32,
}

/// Host-side panel drawer for wasm extension devices.
///
/// Retained UI instances are keyed by [`DeviceDrawContext::device_id`], so two
/// devices of the same type keep independent hover, focus, and drag state. The
/// device type still selects the loaded extension and creates the underlying
/// wasm instance.
pub struct ExtensionPanelHost {
    loaded: Arc<LoadedExtensions>,
    instances: BTreeMap<String, UiInstance>,
}

impl ExtensionPanelHost {
    pub(crate) fn new(loaded: Arc<LoadedExtensions>) -> Self {
        Self {
            loaded,
            instances: BTreeMap::new(),
        }
    }
}

impl DevicePanelHost for ExtensionPanelHost {
    fn draw_device_panel(
        &mut self,
        type_id: &str,
        context: &DeviceDrawContext,
    ) -> Option<PanelDraw> {
        let extension = self.loaded.find_type(type_id)?;
        let module = extension.module.as_ref()?;
        let device_key = context.device_id.as_str();
        if !self.instances.contains_key(device_key) {
            let engine = self.loaded.engines.engine(extension.trust);
            let mut instance = ExtensionInstance::instantiate(
                engine,
                module,
                &extension.manifest.id,
                extension.trust,
            )
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
            Ok(panel) => Some(panel),
            Err(error) => {
                tracing::warn!(
                    extension = %extension.manifest.id,
                    type_id,
                    device_id = %context.device_id,
                    error = %error,
                    "extension panel draw failed"
                );
                self.instances.remove(device_key);
                None
            }
        }
    }
}
