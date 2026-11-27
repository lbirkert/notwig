//! Versioned audio-editor sub-editor contribution contract.
//!
//! The host owns the audio editor, the shared curve editor, the canonical clip
//! data, and the undo boundary. A contribution either binds a host-owned curve
//! to the shared editor or asks the host to run an extension-drawn panel over a
//! bounded audio-editor context.

use notwig_core::sequencer::{AudioClipData, AudioSlice};
use notwig_core::{ClipId, RangeMus, TimeMus, TrackId};
use serde::{Deserialize, Serialize};

use crate::command::{CommandEffect, MAX_COMMAND_EDITS};
use crate::context::ThemeSnapshot;
use crate::panel_frame::{PanelFrame, PanelInput};

/// Extension point used for audio-editor sub-editor contributions.
pub const AUDIO_SUB_EDITOR_POINT: &str = "notwig.audio_editor.sub_editor";
/// Contract id and major version for audio-editor sub-editor contributions.
pub const AUDIO_SUB_EDITOR_CONTRACT: &str = "notwig.audio_editor.sub_editor@1";
/// Maximum length of a sub-editor title.
pub const MAX_AUDIO_SUB_EDITOR_TITLE_BYTES: usize = 64;
/// Maximum length of a custom sub-editor renderer id.
pub const MAX_AUDIO_SUB_EDITOR_RENDERER_BYTES: usize = 128;

/// Canonical audio-clip curve a shared curve sub-editor edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CurveTarget {
    /// Clip gain offset in decibels.
    Gain,
    /// Clip stereo pan in `-1..=1`.
    Pan,
}

impl CurveTarget {
    /// Returns the stable manifest name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Gain => "gain",
            Self::Pan => "pan",
        }
    }
}

/// Value formatting and baseline semantics for a shared curve sub-editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CurveValueFormat {
    /// Decibel offset, such as `+6.00 dB`.
    Decibels,
    /// Stereo pan, such as `L42`, `C`, or `R42`.
    Pan,
}

/// Declares which host-owned curve a sub-editor edits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurveTargetSpec {
    /// Canonical clip curve.
    pub target: CurveTarget,
    /// Lowest value shown by the shared curve editor.
    pub value_min: f64,
    /// Highest value shown by the shared curve editor.
    pub value_max: f64,
    /// Optional reference value drawn as a guide.
    #[serde(default)]
    pub baseline: Option<f64>,
    /// Value formatting used by hover labels.
    pub format: CurveValueFormat,
}

impl CurveTargetSpec {
    /// Validates the value range and baseline.
    pub fn validate(&self) -> Result<(), String> {
        if !self.value_min.is_finite()
            || !self.value_max.is_finite()
            || self.value_min >= self.value_max
        {
            return Err("curve value range must be finite and increasing".into());
        }
        if let Some(baseline) = self.baseline {
            if !baseline.is_finite() {
                return Err("curve baseline must be finite".into());
            }
            if baseline < self.value_min || baseline > self.value_max {
                return Err("curve baseline must lie inside the value range".into());
            }
        }
        Ok(())
    }
}

/// Declares an extension-drawn sub-editor panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomSubEditorSpec {
    /// Stable renderer id registered by the extension at runtime.
    pub renderer: String,
}

impl CustomSubEditorSpec {
    /// Validates the renderer id.
    pub fn validate(&self) -> Result<(), String> {
        if self.renderer.trim().is_empty() {
            return Err("custom renderer id must not be empty".into());
        }
        if self.renderer.len() > MAX_AUDIO_SUB_EDITOR_RENDERER_BYTES {
            return Err("custom renderer id is too long".into());
        }
        Ok(())
    }
}

/// Contribution body for one audio-editor sub-editor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioSubEditorKind {
    /// A shared curve editor over a host-owned clip curve.
    Curve(CurveTargetSpec),
    /// An extension-drawn panel over a bounded audio-editor context.
    Custom(CustomSubEditorSpec),
}

impl AudioSubEditorKind {
    /// Returns the curve target when this is a shared curve sub-editor.
    #[must_use]
    pub const fn curve(&self) -> Option<&CurveTargetSpec> {
        match self {
            Self::Curve(spec) => Some(spec),
            Self::Custom(_) => None,
        }
    }

    /// Returns the custom renderer when this is an extension-drawn sub-editor.
    #[must_use]
    pub const fn custom(&self) -> Option<&CustomSubEditorSpec> {
        match self {
            Self::Curve(_) => None,
            Self::Custom(spec) => Some(spec),
        }
    }
}

/// Resolved audio-editor sub-editor supplied by an extension.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioSubEditorDescriptor {
    /// Stable contribution id.
    pub id: String,
    /// Selector title.
    pub title: String,
    /// Selector category used for ordering.
    pub category: String,
    /// Category-local order. Lower values appear first.
    #[serde(default)]
    pub order: i32,
    /// Sub-editor behavior.
    pub kind: AudioSubEditorKind,
    /// Owning extension id.
    pub extension_id: Option<String>,
}

impl AudioSubEditorDescriptor {
    /// Returns the stable descriptor id.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the selector title.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns the curve target specification when this is a curve editor.
    #[must_use]
    pub fn curve(&self) -> Option<&CurveTargetSpec> {
        self.kind.curve()
    }

    /// Returns the custom renderer specification when this is extension-drawn.
    #[must_use]
    pub fn custom(&self) -> Option<&CustomSubEditorSpec> {
        self.kind.custom()
    }
}

/// Postcard-safe snapshot of an audio clip's editable fields.
///
/// The canonical [`AudioClipData`] uses optional-field serialization defaults
/// that are not round-trippable in postcard. This mirror always encodes every
/// field and converts back to the canonical type for edits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioSubEditorClip {
    /// Source asset.
    pub asset_id: notwig_core::AssetId,
    /// First source sample shown by the clip.
    pub source_start_sample: u64,
    /// One past the last source sample shown by the clip.
    pub source_end_sample: u64,
    /// Clip gain in dB.
    pub gain_db: notwig_core::Db,
    /// Time-varying gain offset in dB.
    pub gain_curve: Option<notwig_core::Curve>,
    /// Time-varying stereo pan in `-1..=1`.
    pub pan_curve: Option<notwig_core::Curve>,
    /// Fade-in length in samples.
    pub fade_in_samples: u64,
    /// Fade-out length in samples.
    pub fade_out_samples: u64,
    /// Fade curve shape.
    pub fade_shape: notwig_core::sequencer::FadeShape,
    /// Audio slices.
    pub slices: Vec<AudioSlice>,
    /// Whether slices drive playback.
    pub slices_enabled: bool,
}

impl From<AudioClipData> for AudioSubEditorClip {
    fn from(audio: AudioClipData) -> Self {
        Self {
            asset_id: audio.asset_id,
            source_start_sample: audio.source_start_sample,
            source_end_sample: audio.source_end_sample,
            gain_db: audio.gain_db,
            gain_curve: audio.gain_curve,
            pan_curve: audio.pan_curve,
            fade_in_samples: audio.fade_in_samples,
            fade_out_samples: audio.fade_out_samples,
            fade_shape: audio.fade_shape,
            slices: audio.slices,
            slices_enabled: audio.slices_enabled,
        }
    }
}

impl From<&AudioClipData> for AudioSubEditorClip {
    fn from(audio: &AudioClipData) -> Self {
        Self {
            asset_id: audio.asset_id,
            source_start_sample: audio.source_start_sample,
            source_end_sample: audio.source_end_sample,
            gain_db: audio.gain_db,
            gain_curve: audio.gain_curve.clone(),
            pan_curve: audio.pan_curve.clone(),
            fade_in_samples: audio.fade_in_samples,
            fade_out_samples: audio.fade_out_samples,
            fade_shape: audio.fade_shape,
            slices: audio.slices.clone(),
            slices_enabled: audio.slices_enabled,
        }
    }
}

impl AudioSubEditorClip {
    /// Converts the snapshot back to canonical clip data.
    #[must_use]
    pub fn into_audio_clip_data(self) -> AudioClipData {
        AudioClipData {
            asset_id: self.asset_id,
            source_start_sample: self.source_start_sample,
            source_end_sample: self.source_end_sample,
            gain_db: self.gain_db,
            gain_curve: self.gain_curve,
            pan_curve: self.pan_curve,
            fade_in_samples: self.fade_in_samples,
            fade_out_samples: self.fade_out_samples,
            fade_shape: self.fade_shape,
            slices: self.slices,
            slices_enabled: self.slices_enabled,
        }
    }
}

/// Bounded audio-editor state supplied to an extension-drawn sub-editor.
///
/// The guest receives this serialized before each draw and must not retain it.
/// The context carries a snapshot, not the project document. Edits are returned
/// in [`CommandEffect`] and only applied after the host validates the revision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioSubEditorContext {
    /// Contribution id of the active sub-editor.
    pub editor_id: String,
    /// Document revision at snapshot time.
    pub revision: u64,
    /// Active audio clip.
    pub clip_id: ClipId,
    /// Track that owns the clip.
    pub track_id: TrackId,
    /// Clip range on the arranger timeline.
    pub clip_range: RangeMus,
    /// Current audio clip snapshot, including slices and host-owned curves.
    pub clip: AudioSubEditorClip,
    /// Selected slices copied from the current clip.
    #[serde(default)]
    pub selected_slices: Vec<AudioSlice>,
    /// Optional time selection in clip-local musical time.
    #[serde(default)]
    pub time_selection: Option<RangeMus>,
    /// Current playhead.
    pub playhead: TimeMus,
    /// Active loop range, when looping is enabled.
    #[serde(default)]
    pub loop_range: Option<RangeMus>,
    /// Project tempo in BPM.
    pub tempo_bpm: f64,
    /// Engine sample rate.
    pub sample_rate: u32,
    /// Whether snapping is currently enabled.
    pub snap_enabled: bool,
    /// Current snap grid step.
    pub grid: TimeMus,
    /// Clip-editor viewport start.
    pub viewport_start: TimeMus,
    /// Clip-editor horizontal zoom.
    pub pixels_per_beat: f32,
    /// Panel width in points.
    pub width: f32,
    /// Panel height in points.
    pub height: f32,
    /// Active host theme.
    pub theme: ThemeSnapshot,
    /// Pointer, keyboard, and focus snapshot for the panel path.
    #[serde(default)]
    pub input: PanelInput,
}

/// One validated extension-drawn audio sub-editor frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioSubEditorFrame {
    /// Tessellated guest UI.
    pub panel: PanelFrame,
    /// Host-owned edits requested by this frame.
    pub effect: CommandEffect,
}

impl AudioSubEditorFrame {
    /// Validates the panel payload and edit budget.
    pub fn validate(&self) -> Result<(), String> {
        self.panel
            .validate()
            .map_err(|error| format!("invalid sub-editor frame: {error}"))?;
        if let CommandEffect::Apply { edits, .. } = &self.effect {
            if edits.len() > MAX_COMMAND_EDITS {
                return Err("sub-editor returned too many edits".into());
            }
        }
        if let CommandEffect::Error { message } = &self.effect {
            if message.len() > 4096 {
                return Err("sub-editor error message is too long".into());
            }
        }
        Ok(())
    }
}

/// Author-facing extension-drawn audio-editor sub-editor.
///
/// The host owns the surrounding editor, time mapping, playhead, snap, focus,
/// validation, and undo. Implementations draw only inside their panel and
/// return host-owned edits. Return [`CommandEffect::None`] on frames that do
/// not commit an edit; the host applies an `Apply` effect as one undo batch.
pub trait AudioSubEditorExtension: Send + Sized + 'static {
    /// Creates a sub-editor instance. One instance is retained per clip and
    /// contribution id while the extension host is loaded.
    fn new() -> Self;

    /// Draws the sub-editor panel and returns requested host edits.
    fn draw(&mut self, ui: &mut egui::Ui, context: &AudioSubEditorContext) -> CommandEffect;
}

/// Host-side sink for extension-drawn audio sub-editors.
pub trait AudioSubEditorHost {
    /// Draws the sub-editor identified by `editor_id` for one audio clip.
    ///
    /// Implementations should retain guest instances keyed by
    /// `(context.clip_id, editor_id)` so switching clips does not destroy view
    /// state. Returning `None` tells the host to draw its fallback placeholder.
    fn draw_audio_sub_editor(
        &mut self,
        editor_id: &str,
        context: &AudioSubEditorContext,
    ) -> Option<AudioSubEditorFrame>;
}
