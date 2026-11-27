//! Bottom-panel extension points shared by the editor shell.

use crate::extension_point::ExtensionPoint;

/// Marker interface for editor bottom-panel implementations.
pub trait BottomPanelInterface {}

/// Bottom panel shown below the device editor.
pub const EP_NOTWIG_DEVICE_EDITOR_BOTTOM_PANEL: ExtensionPoint<dyn BottomPanelInterface> =
    ExtensionPoint::new("notwig.device_editor.bottom_panel");

/// Bottom panel shown below the MIDI editor.
pub const EP_NOTWIG_MIDI_EDITOR_BOTTOM_PANEL: ExtensionPoint<dyn BottomPanelInterface> =
    ExtensionPoint::new("notwig.midi_editor.bottom_panel");

/// Bottom panel shown below the audio clip editor.
pub const EP_NOTWIG_AUDIO_EDITOR_BOTTOM_PANEL: ExtensionPoint<dyn BottomPanelInterface> =
    ExtensionPoint::new("notwig.audio_editor.bottom_panel");
