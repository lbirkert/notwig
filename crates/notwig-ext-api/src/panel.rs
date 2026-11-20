use notwig_core::ParamId;
use serde::{Deserialize, Serialize};

use crate::context::DeviceDrawContext;
use crate::draw::{
    DrawCommand, DrawError, MAX_COMMANDS, MAX_TEXT_BYTES, validate_command, validate_rect,
};
use crate::geometry::Rect;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PanelCommand {
    Draw(DrawCommand),
    Knob {
        rect: Rect,
        param_id: ParamId,
        label: String,
        bipolar: bool,
    },
    Toggle {
        rect: Rect,
        param_id: ParamId,
        label: String,
    },
    Label {
        rect: Rect,
        text: String,
    },
    Section {
        rect: Rect,
        title: String,
    },
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PanelDraw {
    pub commands: Vec<PanelCommand>,
}

impl PanelDraw {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<(), DrawError> {
        if self.commands.len() > MAX_COMMANDS {
            return Err(DrawError::TooManyPanelCommands {
                limit: MAX_COMMANDS,
            });
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

pub trait DevicePanelHost {
    fn draw_device_panel(
        &mut self,
        type_id: &str,
        context: &DeviceDrawContext,
    ) -> Option<PanelDraw>;
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
