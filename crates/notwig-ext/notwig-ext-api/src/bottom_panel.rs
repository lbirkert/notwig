//! Bottom-panel extension point shared by editor shells.

use serde::{Deserialize, Serialize};

use crate::extension_point::ExtensionPoint;

/// Marker interface for bottom-panel implementations.
pub trait BottomPanelInterface {}

/// Editor surface a bottom-panel implementation belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BottomPanelTarget {
    /// Device editor bottom panel.
    DeviceEditor,
    /// MIDI editor bottom panel.
    MidiEditor,
    /// Audio clip editor bottom panel.
    AudioEditor,
}

/// Extension point for the single active editor bottom panel.
///
/// Implementations are distinguished by [`BottomPanelTarget`]. The shell keeps
/// at most one bottom panel open at a time.
pub const EP_NOTWIG_BOTTOM_PANEL: ExtensionPoint<dyn BottomPanelInterface> =
    ExtensionPoint::new("notwig.bottom_panel");
