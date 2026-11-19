use egui::RichText;
use notwig_core::MidiClipData;

use crate::shell::NotwigUi;
use crate::theme;

impl NotwigUi {
    #[allow(clippy::unused_self)]
    pub(super) fn midi_clip_info(&self, ui: &mut egui::Ui, midi: &MidiClipData) {
        ui.label(
            RichText::new(format!("{} notes", midi.notes.len()))
                .small()
                .color(theme::TEXT_MUTED),
        );
    }
}
