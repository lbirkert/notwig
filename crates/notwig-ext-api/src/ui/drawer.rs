use crate::draw::{FontFamily, VerticalAlign};
use crate::geometry::{Color, Point, Rect, Stroke, TextAlign};

/// A font request for [`Drawer::text`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Font {
    /// Font family used for the text.
    pub family: FontFamily,
    /// Nominal font size in panel-local pixels.
    pub size: f32,
}

impl Font {
    /// Creates a font request.
    #[must_use]
    pub const fn new(family: FontFamily, size: f32) -> Self {
        Self { family, size }
    }
}

/// A two-axis text anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Align {
    /// Horizontal alignment relative to the text position.
    pub x: TextAlign,
    /// Vertical alignment relative to the text position.
    pub y: VerticalAlign,
}

impl Align {
    /// Creates a text anchor.
    #[must_use]
    pub const fn new(x: TextAlign, y: VerticalAlign) -> Self {
        Self { x, y }
    }

    /// Top-left aligned text.
    #[must_use]
    pub const fn left_top() -> Self {
        Self::new(TextAlign::Left, VerticalAlign::Top)
    }

    /// Center aligned text.
    #[must_use]
    pub const fn center() -> Self {
        Self::new(TextAlign::Center, VerticalAlign::Center)
    }
}

/// Render-neutral primitive painter used by [`UiContext`](crate::ui::UiContext)
/// and shared widgets.
///
/// Implementations record commands (`RecordDrawer`) or replay them into a host
/// painter (`EguiDrawer`). Coordinates are panel-local pixels.
pub trait Drawer {
    /// Pushes an additional clip rectangle.
    fn push_clip(&mut self, rect: Rect);
    /// Pops the most recent clip rectangle.
    fn pop_clip(&mut self);

    /// Fills a rectangle with an optional corner radius.
    fn fill_rect(&mut self, rect: Rect, radius: f32, color: Color);
    /// Strokes a rectangle with an optional corner radius.
    fn stroke_rect(&mut self, rect: Rect, radius: f32, stroke: Stroke);
    /// Draws a line segment.
    fn line(&mut self, from: Point, to: Point, stroke: Stroke);
    /// Fills a circle.
    fn circle_filled(&mut self, center: Point, radius: f32, color: Color);
    /// Strokes a circle.
    fn circle_stroke(&mut self, center: Point, radius: f32, stroke: Stroke);
    /// Draws text at a two-axis anchor.
    fn text(&mut self, pos: Point, text: &str, font: Font, color: Color, align: Align);
    /// Draws a polyline or polygon.
    fn path(&mut self, points: &[Point], closed: bool, fill: Option<Color>, stroke: Option<Stroke>);
}
