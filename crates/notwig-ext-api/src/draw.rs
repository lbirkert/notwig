use serde::{Deserialize, Serialize};

use crate::geometry::{Color, Point, Rect, Stroke, TextAlign};

pub const MAX_COMMANDS: usize = 4096;
pub const MAX_PATH_POINTS: usize = 1024;
pub const MAX_TEXT_BYTES: usize = 8 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 256 * 1024;
pub const MAX_ACTIONS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FontFamily {
    #[default]
    Proportional,
    Monospace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum VerticalAlign {
    #[default]
    Top,
    Center,
    Bottom,
}

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
    #[error("panel draw exceeds {limit} actions")]
    TooManyActions { limit: usize },
    #[error("action {index} contains a non-finite value")]
    InvalidAction { index: usize },
}

/// A single render-neutral drawing operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DrawCommand {
    /// Pushes an additional clip rectangle.
    PushClip {
        /// Clip rectangle in panel-local pixels.
        rect: Rect,
    },
    /// Pops the most recent clip rectangle.
    PopClip,
    /// Fills a rectangle with an optional corner radius.
    FillRect {
        /// Rectangle to fill.
        rect: Rect,
        /// Corner radius in pixels.
        radius: f32,
        /// Fill color.
        color: Color,
    },
    /// Strokes a rectangle with an optional corner radius.
    StrokeRect {
        /// Rectangle to stroke.
        rect: Rect,
        /// Corner radius in pixels.
        radius: f32,
        /// Stroke width in pixels.
        width: f32,
        /// Stroke color.
        color: Color,
    },
    /// Draws a line segment.
    Line {
        /// Segment start.
        from: Point,
        /// Segment end.
        to: Point,
        /// Line width in pixels.
        width: f32,
        /// Line color.
        color: Color,
    },
    /// Fills a circle.
    CircleFill {
        /// Circle center.
        center: Point,
        /// Circle radius in pixels.
        radius: f32,
        /// Fill color.
        color: Color,
    },
    /// Strokes a circle.
    CircleStroke {
        /// Circle center.
        center: Point,
        /// Circle radius in pixels.
        radius: f32,
        /// Stroke width in pixels.
        width: f32,
        /// Stroke color.
        color: Color,
    },
    /// Draws text at an anchor.
    Text {
        /// Anchor position.
        pos: Point,
        /// Text content.
        text: String,
        /// Nominal font size in pixels.
        size: f32,
        /// Text color.
        color: Color,
        /// Horizontal anchor.
        align: TextAlign,
        /// Font family.
        #[serde(default)]
        family: FontFamily,
        /// Vertical anchor.
        #[serde(default)]
        valign: VerticalAlign,
    },
    /// Draws a polyline or polygon.
    Path {
        /// Path points.
        points: Vec<Point>,
        /// Whether the path closes back to its first point.
        closed: bool,
        /// Optional fill color.
        fill: Option<Color>,
        /// Optional stroke.
        stroke: Option<Stroke>,
    },
}

/// A validated list of drawing operations.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DrawList {
    /// Ordered drawing operations.
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
