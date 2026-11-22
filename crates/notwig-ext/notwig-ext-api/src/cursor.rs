use serde::{Deserialize, Serialize};

/// A neutral cursor request for extension panels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CursorIcon {
    /// The platform default cursor.
    #[default]
    Default,
    /// A pointing hand, for clickable controls.
    PointingHand,
    /// Vertical resize, for vertical drag controls.
    ResizeVertical,
    /// Horizontal resize, for horizontal drag controls.
    ResizeHorizontal,
    /// Text input cursor.
    Text,
    /// Open-hand grab.
    Grab,
    /// Closed-hand grabbing.
    Grabbing,
}
