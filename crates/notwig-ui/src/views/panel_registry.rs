//! Host-side core extension registry for the active editor bottom panel.
//!
//! Built-in panels use the same extension point as package extensions but are
//! owned by the fixed, non-uninstallable `Notwig Core` extension. Implementations
//! are distinguished by [`BottomPanelTarget`]; the shell keeps at most one open.

use egui::Context;
use notwig_core::ProjectDoc;
use notwig_ext_api::{BottomPanelTarget, CORE_EXTENSION_ID, EP_NOTWIG_BOTTOM_PANEL};

use crate::runtime::RuntimeView;
use crate::shell::NotwigUi;
use crate::state::OpenedEditor;

pub(crate) type BottomPanelRenderer = fn(&mut NotwigUi, &Context, &ProjectDoc, &RuntimeView);

pub(crate) struct CoreBottomPanel {
    pub(crate) point: &'static str,
    pub(crate) owner: &'static str,
    pub(crate) id: &'static str,
    pub(crate) name: &'static str,
    pub(crate) target: BottomPanelTarget,
    pub(crate) render: BottomPanelRenderer,
}

pub(crate) const CORE_BOTTOM_PANELS: &[CoreBottomPanel] = &[
    CoreBottomPanel {
        point: EP_NOTWIG_BOTTOM_PANEL.id(),
        owner: CORE_EXTENSION_ID,
        id: "notwig.core.device_editor_bottom_panel",
        name: "Device Editor",
        target: BottomPanelTarget::DeviceEditor,
        render: NotwigUi::device_panel,
    },
    CoreBottomPanel {
        point: EP_NOTWIG_BOTTOM_PANEL.id(),
        owner: CORE_EXTENSION_ID,
        id: "notwig.core.midi_editor_bottom_panel",
        name: "MIDI Editor",
        target: BottomPanelTarget::MidiEditor,
        render: NotwigUi::midi_editor_panel,
    },
    CoreBottomPanel {
        point: EP_NOTWIG_BOTTOM_PANEL.id(),
        owner: CORE_EXTENSION_ID,
        id: "notwig.core.audio_editor_bottom_panel",
        name: "Audio Editor",
        target: BottomPanelTarget::AudioEditor,
        render: NotwigUi::clip_editor_panel,
    },
];

fn editor_target(editor: OpenedEditor) -> BottomPanelTarget {
    match editor {
        OpenedEditor::DeviceEditor => BottomPanelTarget::DeviceEditor,
        OpenedEditor::MidiEditor => BottomPanelTarget::MidiEditor,
        OpenedEditor::ClipEditor => BottomPanelTarget::AudioEditor,
    }
}

pub(crate) fn core_bottom_panel(editor: OpenedEditor) -> Option<&'static CoreBottomPanel> {
    let target = editor_target(editor);
    CORE_BOTTOM_PANELS
        .iter()
        .find(|panel| panel.target == target)
}

pub mod test_api {
    use super::CORE_BOTTOM_PANELS;
    use notwig_ext_api::BottomPanelTarget;

    /// Returns `(point, owner, implementation id, name, target)` for every
    /// built-in bottom-panel implementation.
    #[must_use]
    pub fn core_bottom_panel_descriptors() -> Vec<(
        &'static str,
        &'static str,
        &'static str,
        &'static str,
        BottomPanelTarget,
    )> {
        CORE_BOTTOM_PANELS
            .iter()
            .map(|panel| (panel.point, panel.owner, panel.id, panel.name, panel.target))
            .collect()
    }
}
