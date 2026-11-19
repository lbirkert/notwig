use egui::RichText;
use notwig_core::{AudioClipData, ProjectDoc};

use crate::theme;

pub(super) fn audio_source_info(ui: &mut egui::Ui, doc: &ProjectDoc, audio: &AudioClipData) {
    let sample_rate = f64::from(doc.meta.sample_rate.value());
    ui.label(
        RichText::new(format!(
            "{:.2} s · fades {:.2} / {:.2} s",
            audio.source_duration() as f64 / sample_rate,
            audio.fade_in_samples as f64 / sample_rate,
            audio.fade_out_samples as f64 / sample_rate,
        ))
        .small()
        .color(theme::TEXT_MUTED),
    );
}
