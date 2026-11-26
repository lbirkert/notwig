//! Modifier-aware drag offsets with dynamic fine adjustment.

/// Tracks one drag axis and switches between coarse and fine offsets without
/// losing the motion that happened before Shift was pressed.
#[derive(Debug, Clone, Default)]
pub struct FineDrag {
    started: bool,
    shift: bool,
    anchor: Option<f32>,
    base: f64,
    last: f64,
}

/// Result of one drag update.
#[derive(Debug, Clone, Copy, Default)]
pub struct FineDragUpdate {
    /// Effective offset for this frame.
    pub offset: f64,
    /// Raw pointer delta from the drag origin.
    pub coarse: f64,
    /// Shift was pressed this frame.
    pub shift_started: bool,
    /// The drag began with Shift already held.
    pub fine_from_start: bool,
}

impl FineDrag {
    /// Updates the drag with a raw axis delta from the press origin.
    pub fn update(&mut self, raw_delta: f32, shift: bool, fine_ratio: f64) -> FineDragUpdate {
        let coarse = f64::from(raw_delta);
        let mut shift_started = false;
        let mut fine_from_start = false;
        if !self.started {
            self.started = true;
            self.shift = shift;
            self.anchor = Some(0.0);
            self.base = 0.0;
            fine_from_start = shift;
        } else if shift != self.shift {
            self.shift = shift;
            self.anchor = Some(raw_delta);
            self.base = self.last;
            shift_started = true;
        }
        let anchor = self.anchor.unwrap_or(0.0);
        let ratio = if self.shift { fine_ratio } else { 1.0 };
        let offset = self.base + f64::from(raw_delta - anchor) * ratio;
        self.last = offset;
        FineDragUpdate {
            offset,
            coarse,
            shift_started,
            fine_from_start,
        }
    }

    /// Overrides the frozen coarse base, for callers that snap their coarse
    /// offsets. Returns the new effective offset.
    pub fn set_base(&mut self, offset: f64) -> f64 {
        self.base = offset;
        self.last = offset;
        offset
    }

    /// Returns the last effective offset.
    pub fn offset(&self) -> f64 {
        self.last
    }

    /// Returns whether Shift is currently active for this drag.
    pub fn shift(&self) -> bool {
        self.shift
    }
}

#[doc(hidden)]
pub mod test_api {
    pub use super::{FineDrag, FineDragUpdate};
}
