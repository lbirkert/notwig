//! Host-side core extension registry for editor bottom panels.
//!
//! Built-in panels use the same extension-point ids as package extensions but
//! are owned by the fixed, non-uninstallable `Notwig Core` extension.

use egui::Context;
use notwig_core::ProjectDoc;
use notwig_ext_api::{
    CORE_EXTENSION_ID, EP_NOTWIG_AUDIO_EDITOR_BOTTOM_PANEL, EP_NOTWIG_DEVICE_EDITOR_BOTTOM_PANEL,
    EP_NOTWIG_MIDI_EDITOR_BOTTOM_PANEL,
};

use crate::runtime::RuntimeView;
use crate::shell::NotwigUi;
use crate::state::OpenedEditor;

pub(crate) type BottomPanelRenderer = fn(&mut NotwigUi, &Context, &ProjectDoc, &RuntimeView);

pub(crate) struct CoreBottomPanel {
    pub(crate) point: &'static str,
    pub(crate) owner: &'static str,
    pub(crate) name: &'static str,
    pub(crate) render: BottomPanelRenderer,
}

pub(crate) const CORE_BOTTOM_PANELS: &[CoreBottomPanel] = &[
    CoreBottomPanel {
        point: EP_NOTWIG_DEVICE_EDITOR_BOTTOM_PANEL.id(),
        owner: CORE_EXTENSION_ID,
        name: "Device Editor",
        render: NotwigUi::device_panel,
    },
    CoreBottomPanel {
        point: EP_NOTWIG_MIDI_EDITOR_BOTTOM_PANEL.id(),
        owner: CORE_EXTENSION_ID,
        name: "MIDI Editor",
        render: NotwigUi::midi_editor_panel,
    },
    CoreBottomPanel {
        point: EP_NOTWIG_AUDIO_EDITOR_BOTTOM_PANEL.id(),
        owner: CORE_EXTENSION_ID,
        name: "Audio Editor",
        render: NotwigUi::clip_editor_panel,
    },
];

pub(crate) fn bottom_panel_point(editor: OpenedEditor) -> &'static str {
    match editor {
        OpenedEditor::DeviceEditor => EP_NOTWIG_DEVICE_EDITOR_BOTTOM_PANEL.id(),
        OpenedEditor::MidiEditor => EP_NOTWIG_MIDI_EDITOR_BOTTOM_PANEL.id(),
        OpenedEditor::ClipEditor => EP_NOTWIG_AUDIO_EDITOR_BOTTOM_PANEL.id(),
    }
}

pub(crate) fn core_bottom_panel(editor: OpenedEditor) -> Option<&'static CoreBottomPanel> {
    let point = bottom_panel_point(editor);
    CORE_BOTTOM_PANELS.iter().find(|panel| panel.point == point)
}

pub mod test_api {
    use super::CORE_BOTTOM_PANELS;

    /// Returns `(point, owner, name)` for every built-in bottom-panel implementation.
    #[must_use]
    pub fn core_bottom_panel_descriptors() -> Vec<(&'static str, &'static str, &'static str)> {
        CORE_BOTTOM_PANELS
            .iter()
            .map(|panel| (panel.point, panel.owner, panel.name))
            .collect()
    }
}
