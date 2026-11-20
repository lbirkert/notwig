use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    #[must_use]
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }

    #[must_use]
    pub fn distance(self, other: Self) -> f32 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}

impl std::ops::Add for Point {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl std::ops::Sub for Point {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    #[must_use]
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    #[must_use]
    pub fn is_finite(self) -> bool {
        self.width.is_finite() && self.height.is_finite()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Rect {
    pub min: Point,
    pub max: Point,
}

impl Rect {
    #[must_use]
    pub const fn new(min: Point, max: Point) -> Self {
        Self { min, max }
    }

    #[must_use]
    pub const fn from_min_size(min: Point, size: Size) -> Self {
        Self {
            min,
            max: Point::new(min.x + size.width, min.y + size.height),
        }
    }

    #[must_use]
    pub fn width(self) -> f32 {
        self.max.x - self.min.x
    }

    #[must_use]
    pub fn height(self) -> f32 {
        self.max.y - self.min.y
    }

    #[must_use]
    pub fn size(self) -> Size {
        Size::new(self.width(), self.height())
    }

    #[must_use]
    pub fn is_finite(self) -> bool {
        self.min.is_finite() && self.max.is_finite()
    }

    #[must_use]
    pub fn is_ordered(self) -> bool {
        self.min.x <= self.max.x && self.min.y <= self.max.y
    }

    #[must_use]
    pub fn is_valid(self) -> bool {
        self.is_finite() && self.is_ordered()
    }

    #[must_use]
    pub fn contains(self, point: Point) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
    }

    #[must_use]
    pub fn intersect(self, other: Self) -> Self {
        Self::new(
            Point::new(self.min.x.max(other.min.x), self.min.y.max(other.min.y)),
            Point::new(self.max.x.min(other.max.x), self.max.y.min(other.max.y)),
        )
    }

    #[must_use]
    pub fn center(self) -> Point {
        Point::new(
            f32::midpoint(self.min.x, self.max.x),
            f32::midpoint(self.min.y, self.max.y),
        )
    }

    #[must_use]
    pub fn shrink(self, amount: f32) -> Self {
        Self::new(
            Point::new(self.min.x + amount, self.min.y + amount),
            Point::new(self.max.x - amount, self.max.y - amount),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const TRANSPARENT: Self = Self::rgba(0, 0, 0, 0);
    pub const BLACK: Self = Self::rgba(0, 0, 0, 255);
    pub const WHITE: Self = Self::rgba(255, 255, 255, 255);

    #[must_use]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::rgba(r, g, b, 255)
    }

    #[must_use]
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::TRANSPARENT
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Stroke {
    pub width: f32,
    pub color: Color,
}

impl Stroke {
    #[must_use]
    pub const fn new(width: f32, color: Color) -> Self {
        Self { width, color }
    }

    #[must_use]
    pub fn is_valid(self) -> bool {
        self.width.is_finite() && self.width >= 0.0
    }
}
