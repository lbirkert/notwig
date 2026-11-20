use notwig_core::{ParamId, ParamValue};
use serde::{Deserialize, Serialize};

use crate::context::DeviceDrawContext;
use crate::draw::{
    DrawCommand, DrawError, MAX_ACTIONS, MAX_COMMANDS, MAX_TEXT_BYTES, validate_command,
    validate_rect,
};
use crate::geometry::Rect;
use crate::ui::CursorIcon;

/// A single panel drawing or host-widget operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PanelCommand {
    /// A render-neutral primitive drawing command.
    Draw(DrawCommand),
    /// A host-rendered parameter knob.
    Knob {
        /// Rectangle occupied by the knob.
        rect: Rect,
        /// Parameter controlled by the knob.
        param_id: ParamId,
        /// Display label.
        label: String,
        /// Whether the knob is drawn around its center.
        bipolar: bool,
    },
    /// A host-rendered parameter toggle.
    Toggle {
        /// Rectangle occupied by the toggle.
        rect: Rect,
        /// Parameter controlled by the toggle.
        param_id: ParamId,
        /// Display label.
        label: String,
    },
    /// A host-rendered label.
    Label {
        /// Rectangle occupied by the label.
        rect: Rect,
        /// Label text.
        text: String,
    },
    /// A host-rendered section header.
    Section {
        /// Rectangle occupied by the header.
        rect: Rect,
        /// Header title.
        title: String,
    },
}

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

/// One panel frame: ordered draw/widget commands plus host actions.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PanelDraw {
    /// Ordered draw and host-widget commands.
    pub commands: Vec<PanelCommand>,
    /// Host actions requested during the frame.
    #[serde(default)]
    pub actions: Vec<PanelAction>,
}

impl PanelDraw {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            commands: Vec::new(),
            actions: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<(), DrawError> {
        if self.commands.len() > MAX_COMMANDS {
            return Err(DrawError::TooManyPanelCommands {
                limit: MAX_COMMANDS,
            });
        }
        if self.actions.len() > MAX_ACTIONS {
            return Err(DrawError::TooManyActions { limit: MAX_ACTIONS });
        }
        for (index, action) in self.actions.iter().enumerate() {
            validate_action(action, index)?;
        }
        for (index, command) in self.commands.iter().enumerate() {
            match command {
                PanelCommand::Draw(command) => validate_command(command, index)?,
                PanelCommand::Knob { rect, label, .. }
                | PanelCommand::Toggle { rect, label, .. } => {
                    validate_rect(*rect, index)?;
                    validate_text(label, index)?;
                }
                PanelCommand::Label { rect, text } => {
                    validate_rect(*rect, index)?;
                    validate_text(text, index)?;
                }
                PanelCommand::Section { rect, title } => {
                    validate_rect(*rect, index)?;
                    validate_text(title, index)?;
                }
            }
        }
        Ok(())
    }
}

/// Host-side sink for extension device panels.
///
/// Implementations should key retained UI state by
/// [`DeviceDrawContext::device_id`], not by device type. The context carries a
/// per-frame input snapshot and theme.
pub trait DevicePanelHost {
    /// Draws the panel for one device instance.
    fn draw_device_panel(
        &mut self,
        type_id: &str,
        context: &DeviceDrawContext,
    ) -> Option<PanelDraw>;
}

fn validate_action(action: &PanelAction, index: usize) -> Result<(), DrawError> {
    match action {
        PanelAction::SetParamNormalized { norm, .. } => {
            if !norm.is_finite() {
                return Err(DrawError::InvalidAction { index });
            }
            Ok(())
        }
        PanelAction::SetParamValue { value, .. } => {
            if let ParamValue::F64(value) = value {
                if !value.is_finite() {
                    return Err(DrawError::InvalidAction { index });
                }
            }
            Ok(())
        }
        PanelAction::SetCursor { .. } | PanelAction::RequestRepaint => Ok(()),
    }
}

fn validate_text(text: &str, index: usize) -> Result<(), DrawError> {
    if text.len() > MAX_TEXT_BYTES {
        return Err(DrawError::TextTooLong {
            index,
            limit: MAX_TEXT_BYTES,
        });
    }
    Ok(())
}
