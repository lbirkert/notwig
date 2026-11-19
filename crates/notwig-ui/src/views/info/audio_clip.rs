use notwig_core::{AudioClipData, ProjectDoc};

use crate::shell::NotwigUi;

use super::files;

impl NotwigUi {
    #[allow(clippy::unused_self)]
    pub(super) fn audio_clip_info(
        &self,
        ui: &mut egui::Ui,
        doc: &ProjectDoc,
        audio: &AudioClipData,
    ) {
        files::audio_source_info(ui, doc, audio);
    }
}
