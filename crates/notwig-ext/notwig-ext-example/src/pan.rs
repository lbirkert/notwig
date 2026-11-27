//! Extension-drawn clip pan sub-editor.

use notwig_core::Curve;
use notwig_ext_api::{
    AudioSubEditorContext, AudioSubEditorDescriptor, AudioSubEditorExtension,
    AudioSubEditorResponse, CommandEdit, CommandEffect,
};

/// Clip pan sub-editor id.
pub const PAN_SUB_EDITOR_ID: &str = "notwig.ext.example.pan";

/// Returns the clip-pan sub-editor metadata.
#[must_use]
pub fn descriptor() -> AudioSubEditorDescriptor {
    AudioSubEditorDescriptor {
        id: PAN_SUB_EDITOR_ID.into(),
        title: "Clip Pan".into(),
        category: "Audio".into(),
        order: 10,
        extension_id: None,
    }
}

/// Extension-drawn clip pan sub-editor.
pub struct PanSubEditor {
    selected: std::collections::BTreeSet<usize>,
    time_selection: Option<notwig_core::RangeMus>,
    state: notwig_ui::extension_ui::LineEditorState,
}

impl AudioSubEditorExtension for PanSubEditor {
    fn new() -> Self {
        Self {
            selected: std::collections::BTreeSet::new(),
            time_selection: None,
            state: notwig_ui::extension_ui::LineEditorState::default(),
        }
    }

    fn descriptor() -> AudioSubEditorDescriptor {
        descriptor()
    }

    fn draw(
        &mut self,
        ui: &mut notwig_ext_api::egui::Ui,
        context: &AudioSubEditorContext,
    ) -> AudioSubEditorResponse {
        let mut curve = context.clip.pan_curve.clone().unwrap_or_default();
        let edit = notwig_ui::extension_ui::apply_curve_edit_actions(
            &mut curve,
            &mut self.selected,
            &mut self.time_selection,
            &context.actions,
            &context.clipboard,
        );
        let timeline = notwig_ui::extension_ui::TimelineGrid {
            start: context.viewport_start - context.clip_range.start,
            pixels_per_beat: context.pixels_per_beat,
            bar_length: context.bar_length,
            beat_length: context.beat_length,
            step: context.grid,
            snap_enabled: context.snap_enabled,
        };
        let config = notwig_ui::extension_ui::LineEditorConfig {
            value_min: -1.0,
            value_max: 1.0,
            baseline: Some(0.0),
            min_time: None,
            max_time: None,
            format_value: Some(format_pan),
        };
        let rect = notwig_ext_api::egui::Rect::from_min_size(
            notwig_ext_api::egui::Pos2::ZERO,
            notwig_ext_api::egui::vec2(context.width, context.height),
        );
        let response = notwig_ui::extension_ui::show_with_background(
            ui,
            rect,
            notwig_ext_api::egui::Id::new((
                "notwig.audio-sub-editor",
                context.editor_id.as_str(),
                context.clip_id,
            )),
            timeline,
            &config,
            &mut curve,
            &mut self.selected,
            &mut self.time_selection,
            &mut self.state,
            &mut |ui, _curve| {
                notwig_ui::extension_ui::draw_waveform_preview(
                    ui.painter(),
                    rect,
                    &context.waveform,
                );
            },
        );
        let effect = if edit.changed || response.committed {
            pan_effect(context, &curve)
        } else {
            CommandEffect::None
        };
        AudioSubEditorResponse {
            effect,
            clipboard: edit.copied,
        }
    }
}

fn pan_effect(context: &AudioSubEditorContext, curve: &Curve) -> CommandEffect {
    CommandEffect::Apply {
        edits: vec![CommandEdit::AudioCurves {
            clip_id: context.clip_id,
            gain_curve: None,
            pan_curve: Some(Some(curve.clone())),
        }],
        ui: Vec::new(),
    }
}

fn format_pan(value: f64) -> String {
    let amount = (value.clamp(-1.0, 1.0).abs() * 100.0).round() as i32;
    if amount == 0 {
        "C".into()
    } else if value < 0.0 {
        format!("L{amount}")
    } else {
        format!("R{amount}")
    }
}
