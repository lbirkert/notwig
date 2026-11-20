use std::collections::BTreeMap;
use std::sync::Arc;

use notwig_ext_api::{DeviceDrawContext, DevicePanelHost, PanelDraw};

use crate::abi::ExtensionInstance;
use crate::compiled::LoadedExtensions;

struct UiInstance {
    instance: ExtensionInstance,
    handle: i32,
}

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
        if !self.instances.contains_key(type_id) {
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
                    error = %error,
                    "could not create extension panel instance"
                );
                error
            })
            .ok()?;
            let handle = instance.create_device(type_id).ok()?;
            for (param_id, value) in &context.params {
                let _ = instance.set_param(handle, *param_id, value);
            }
            self.instances
                .insert(type_id.to_owned(), UiInstance { instance, handle });
        }
        let result = {
            let ui = self.instances.get_mut(type_id)?;
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
                    error = %error,
                    "extension panel draw failed"
                );
                self.instances.remove(type_id);
                None
            }
        }
    }
}
