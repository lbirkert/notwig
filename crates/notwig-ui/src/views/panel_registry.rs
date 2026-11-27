//! Host-side core extension registry for the active editor bottom panel.
//!
//! Built-in panels use the same extension point as package extensions but are
//! owned by the fixed, non-uninstallable `Notwig Core` extension. Implementations
//! are distinguished by [`BottomPanelTarget`]; the shell keeps at most one open.

use egui::Context;
use notwig_core::ProjectDoc;
use notwig_ext_api::{CORE_EXTENSION_ID, EP_NOTWIG_BOTTOM_PANEL};

use crate::runtime::RuntimeView;
use crate::shell::NotwigUi;
use crate::state::OpenedEditor;

pub(crate) type BottomPanelRenderer = fn(&mut NotwigUi, &Context, &ProjectDoc, &RuntimeView);

pub(crate) struct CoreBottomPanel {
    pub(crate) point: &'static str,
    pub(crate) owner: &'static str,
    pub(crate) id: &'static str,
    pub(crate) name: &'static str,
    pub(crate) render: BottomPanelRenderer,
}

pub(crate) const CORE_BOTTOM_PANELS: &[CoreBottomPanel] = &[
    CoreBottomPanel {
        point: EP_NOTWIG_BOTTOM_PANEL.id(),
        owner: CORE_EXTENSION_ID,
        id: "notwig.core.device_editor_bottom_panel",
        name: "Device Editor",
        render: NotwigUi::device_panel,
    },
    CoreBottomPanel {
        point: EP_NOTWIG_BOTTOM_PANEL.id(),
        owner: CORE_EXTENSION_ID,
        id: "notwig.core.midi_editor_bottom_panel",
        name: "MIDI Editor",
        render: NotwigUi::midi_editor_panel,
    },
    CoreBottomPanel {
        point: EP_NOTWIG_BOTTOM_PANEL.id(),
        owner: CORE_EXTENSION_ID,
        id: "notwig.core.audio_editor_bottom_panel",
        name: "Audio Editor",
        render: NotwigUi::clip_editor_panel,
    },
];

pub(crate) fn core_panel_id(editor: OpenedEditor) -> &'static str {
    match editor {
        OpenedEditor::DeviceEditor => "notwig.core.device_editor_bottom_panel",
        OpenedEditor::MidiEditor => "notwig.core.midi_editor_bottom_panel",
        OpenedEditor::ClipEditor => "notwig.core.audio_editor_bottom_panel",
    }
}

pub(crate) fn core_bottom_panel(editor: OpenedEditor) -> Option<&'static CoreBottomPanel> {
    let id = core_panel_id(editor);
    CORE_BOTTOM_PANELS.iter().find(|panel| panel.id == id)
}

pub mod test_api {
    use super::CORE_BOTTOM_PANELS;

    /// Returns `(point, owner, implementation id, name)` for every built-in
    /// bottom-panel implementation.
    #[must_use]
    pub fn core_bottom_panel_descriptors()
    -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
        CORE_BOTTOM_PANELS
            .iter()
            .map(|panel| (panel.point, panel.owner, panel.id, panel.name))
            .collect()
    }
}
