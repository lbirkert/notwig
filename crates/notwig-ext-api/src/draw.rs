use serde::{Deserialize, Serialize};

use crate::geometry::{Color, Point, Rect, Stroke, TextAlign};

pub const MAX_COMMANDS: usize = 4096;
pub const MAX_PATH_POINTS: usize = 1024;
pub const MAX_TEXT_BYTES: usize = 8 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 256 * 1024;

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum DrawError {
    #[error("draw list exceeds {limit} commands")]
    TooManyCommands { limit: usize },
    #[error("path at command {index} exceeds {limit} points")]
    TooManyPoints { index: usize, limit: usize },
    #[error("text at command {index} exceeds {limit} bytes")]
    TextTooLong { index: usize, limit: usize },
    #[error("command {index} contains non-finite geometry")]
    NonFinite { index: usize },
    #[error("command {index} has an invalid rectangle")]
    InvalidRect { index: usize },
    #[error("command {index} has an invalid stroke")]
    InvalidStroke { index: usize },
    #[error("panel draw exceeds {limit} commands")]
    TooManyPanelCommands { limit: usize },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DrawCommand {
    PushClip {
        rect: Rect,
    },
    PopClip,
    FillRect {
        rect: Rect,
        radius: f32,
        color: Color,
    },
    StrokeRect {
        rect: Rect,
        radius: f32,
        width: f32,
        color: Color,
    },
    Line {
        from: Point,
        to: Point,
        width: f32,
        color: Color,
    },
    CircleFill {
        center: Point,
        radius: f32,
        color: Color,
    },
    CircleStroke {
        center: Point,
        radius: f32,
        width: f32,
        color: Color,
    },
    Text {
        pos: Point,
        text: String,
        size: f32,
        color: Color,
        align: TextAlign,
    },
    Path {
        points: Vec<Point>,
        closed: bool,
        fill: Option<Color>,
        stroke: Option<Stroke>,
    },
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DrawList {
    pub commands: Vec<DrawCommand>,
}

impl DrawList {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<(), DrawError> {
        if self.commands.len() > MAX_COMMANDS {
            return Err(DrawError::TooManyCommands {
                limit: MAX_COMMANDS,
            });
        }
        for (index, command) in self.commands.iter().enumerate() {
            validate_command(command, index)?;
        }
        Ok(())
    }
}

pub(crate) fn validate_command(command: &DrawCommand, index: usize) -> Result<(), DrawError> {
    match command {
        DrawCommand::PushClip { rect } => validate_rect(*rect, index),
        DrawCommand::PopClip => Ok(()),
        DrawCommand::FillRect { rect, radius, .. } => {
            validate_rect(*rect, index)?;
            validate_non_negative(*radius, index)
        }
        DrawCommand::StrokeRect {
            rect,
            radius,
            width,
            ..
        } => {
            validate_rect(*rect, index)?;
            validate_non_negative(*radius, index)?;
            validate_non_negative(*width, index)
        }
        DrawCommand::Line {
            from, to, width, ..
        } => {
            if !from.is_finite() || !to.is_finite() {
                return Err(DrawError::NonFinite { index });
            }
            validate_non_negative(*width, index)
        }
        DrawCommand::CircleFill { center, radius, .. } => {
            if !center.is_finite() {
                return Err(DrawError::NonFinite { index });
            }
            validate_non_negative(*radius, index)
        }
        DrawCommand::CircleStroke {
            center,
            radius,
            width,
            ..
        } => {
            if !center.is_finite() {
                return Err(DrawError::NonFinite { index });
            }
            validate_non_negative(*radius, index)?;
            validate_non_negative(*width, index)
        }
        DrawCommand::Text {
            pos, text, size, ..
        } => {
            if !pos.is_finite() || !size.is_finite() || *size < 0.0 {
                return Err(DrawError::NonFinite { index });
            }
            if text.len() > MAX_TEXT_BYTES {
                return Err(DrawError::TextTooLong {
                    index,
                    limit: MAX_TEXT_BYTES,
                });
            }
            Ok(())
        }
        DrawCommand::Path { points, stroke, .. } => {
            if points.len() > MAX_PATH_POINTS {
                return Err(DrawError::TooManyPoints {
                    index,
                    limit: MAX_PATH_POINTS,
                });
            }
            if points.iter().any(|point| !point.is_finite()) {
                return Err(DrawError::NonFinite { index });
            }
            if let Some(stroke) = stroke {
                if !stroke.is_valid() {
                    return Err(DrawError::InvalidStroke { index });
                }
            }
            Ok(())
        }
    }
}

pub(crate) fn validate_rect(rect: Rect, index: usize) -> Result<(), DrawError> {
    if !rect.is_finite() {
        return Err(DrawError::NonFinite { index });
    }
    if !rect.is_ordered() {
        return Err(DrawError::InvalidRect { index });
    }
    Ok(())
}

pub(crate) fn validate_non_negative(value: f32, index: usize) -> Result<(), DrawError> {
    if !value.is_finite() || value < 0.0 {
        return Err(DrawError::NonFinite { index });
    }
    Ok(())
}
