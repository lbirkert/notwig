use notwig_core::{ParamId, ParamValue};
use serde::{Deserialize, Serialize};

use crate::context::DeviceDrawContext;
use crate::cursor::CursorIcon;
use crate::panel_frame::PanelFrame;

/// A host-side action requested by an extension panel.
///
/// Actions are validated against the device spec before the host applies them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PanelAction {
    /// Set a parameter using its normalized `0..=1` value.
    SetParamNormalized {
        /// Parameter to change.
        param_id: ParamId,
        /// Normalized value, clamped by the host.
        norm: f32,
    },
    /// Set a parameter to an exact value.
    SetParamValue {
        /// Parameter to change.
        param_id: ParamId,
        /// Exact value, validated against the parameter kind and range.
        value: ParamValue,
    },
    /// Request a cursor icon for the panel surface.
    SetCursor {
        /// Requested cursor.
        cursor: CursorIcon,
    },
    /// Request another UI frame.
    RequestRepaint,
}

/// Host-side sink for extension device panels.
///
/// Implementations should key retained UI state by
/// [`DeviceDrawContext::device_id`], not by device type. The context carries
/// per-frame input snapshots and theme.
pub trait DevicePanelHost {
    /// Draws the egui panel for one device instance.
    fn draw_device_panel(
        &mut self,
        type_id: &str,
        context: &DeviceDrawContext,
    ) -> Option<PanelFrame>;
}

pub(crate) fn validate_action(action: &PanelAction) -> Result<(), &'static str> {
    match action {
        PanelAction::SetParamNormalized { norm, .. } => {
            if norm.is_finite() {
                Ok(())
            } else {
                Err("normalized value is not finite")
            }
        }
        PanelAction::SetParamValue { value, .. } => {
            if let ParamValue::F64(value) = value {
                if !value.is_finite() {
                    return Err("parameter value is not finite");
                }
            }
            Ok(())
        }
        PanelAction::SetCursor { .. } | PanelAction::RequestRepaint => Ok(()),
    }
}
