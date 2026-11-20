use crate::draw::DrawCommand;
use crate::geometry::{Color, Point, Rect, Stroke};

use super::drawer::{Align, Drawer, Font};

/// A [`Drawer`] that buffers commands for the WASM/host boundary.
///
/// This is the buffered drawer used by extension device panels. It performs no
/// I/O and has no host dependency.
#[derive(Debug, Clone, Default)]
pub struct RecordDrawer {
    commands: Vec<DrawCommand>,
}

impl RecordDrawer {
    /// Creates an empty recording drawer.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    /// Consumes the drawer and returns the recorded draw commands.
    #[must_use]
    pub fn into_commands(self) -> Vec<DrawCommand> {
        self.commands
    }
}

impl Drawer for RecordDrawer {
    fn push_clip(&mut self, rect: Rect) {
        self.commands.push(DrawCommand::PushClip { rect });
    }

    fn pop_clip(&mut self) {
        self.commands.push(DrawCommand::PopClip);
    }

    fn fill_rect(&mut self, rect: Rect, radius: f32, color: Color) {
        self.commands.push(DrawCommand::FillRect {
            rect,
            radius,
            color,
        });
    }

    fn stroke_rect(&mut self, rect: Rect, radius: f32, stroke: Stroke) {
        self.commands.push(DrawCommand::StrokeRect {
            rect,
            radius,
            width: stroke.width,
            color: stroke.color,
        });
    }

    fn line(&mut self, from: Point, to: Point, stroke: Stroke) {
        self.commands.push(DrawCommand::Line {
            from,
            to,
            width: stroke.width,
            color: stroke.color,
        });
    }

    fn circle_filled(&mut self, center: Point, radius: f32, color: Color) {
        self.commands.push(DrawCommand::CircleFill {
            center,
            radius,
            color,
        });
    }

    fn circle_stroke(&mut self, center: Point, radius: f32, stroke: Stroke) {
        self.commands.push(DrawCommand::CircleStroke {
            center,
            radius,
            width: stroke.width,
            color: stroke.color,
        });
    }

    fn text(&mut self, pos: Point, text: &str, font: Font, color: Color, align: Align) {
        self.commands.push(DrawCommand::Text {
            pos,
            text: text.to_owned(),
            size: font.size,
            color,
            align: align.x,
            family: font.family,
            valign: align.y,
        });
    }

    fn path(
        &mut self,
        points: &[Point],
        closed: bool,
        fill: Option<Color>,
        stroke: Option<Stroke>,
    ) {
        self.commands.push(DrawCommand::Path {
            points: points.to_vec(),
            closed,
            fill,
            stroke,
        });
    }
}
