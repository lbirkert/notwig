use std::collections::BTreeMap;
use std::sync::Arc;

use notwig_ext_api::{DeviceDrawContext, DevicePanelFrame, DevicePanelHost, ExtensionCapability};

use crate::compiled::LoadedExtensions;
use crate::instance::ExtensionInstance;

struct UiInstance {
    instance: ExtensionInstance,
    handle: i32,
    supports_egui: bool,
}

/// Host-side panel drawer for extension devices.
///
/// Retained UI instances are keyed by [`DeviceDrawContext::device_id`], so two
/// devices of the same type keep independent hover, focus, and drag state. The
/// device type still selects the loaded extension and creates the underlying
/// WASM or native instance.
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
    ) -> Option<DevicePanelFrame> {
        let extension = self.loaded.find_type(type_id)?;
        let runtime = extension.runtime.as_ref()?;
        let device_key = context.device_id.as_str();
        if !self.instances.contains_key(device_key) {
            let mut instance = ExtensionInstance::instantiate(
                runtime,
                &self.loaded.engines,
                &extension.manifest.id,
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
            let supports_egui = extension
                .manifest
                .has_capability(ExtensionCapability::UiEgui)
                && instance.supports_egui(type_id).unwrap_or(false);
            self.instances.insert(
                context.device_id.clone(),
                UiInstance {
                    instance,
                    handle,
                    supports_egui,
                },
            );
        }
        let result = {
            let ui = self.instances.get_mut(device_key)?;
            for (param_id, value) in &context.params {
                let _ = ui.instance.set_param(ui.handle, *param_id, value);
            }
            if ui.supports_egui {
                match ui.instance.draw_egui_panel(ui.handle, context) {
                    Ok(frame) => Ok(DevicePanelFrame::Egui(frame)),
                    Err(error) => {
                        tracing::warn!(
                            extension = %extension.manifest.id,
                            type_id,
                            device_id = %context.device_id,
                            error = %error,
                            "extension egui panel failed; falling back to command panels"
                        );
                        ui.supports_egui = false;
                        ui.instance
                            .draw_panel(ui.handle, context)
                            .map(DevicePanelFrame::Commands)
                    }
                }
            } else {
                ui.instance
                    .draw_panel(ui.handle, context)
                    .map(DevicePanelFrame::Commands)
            }
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
                self.instances.remove(device_key);
                None
            }
        }
    }
}
