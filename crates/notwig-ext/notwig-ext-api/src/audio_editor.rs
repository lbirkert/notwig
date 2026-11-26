//! Versioned audio-editor sub-editor contribution contract.
//!
//! The host owns the audio editor, the shared curve editor, the canonical clip
//! data, and the undo boundary. A contribution adds a sub-editor to the audio
//! editor selector. Curve contributions bind a host-owned audio-clip curve to
//! the shared curve editor; the extension does not draw or store the curve.

use serde::{Deserialize, Serialize};

/// Extension point used for audio-editor sub-editor contributions.
pub const AUDIO_SUB_EDITOR_POINT: &str = "notwig.audio_editor.sub_editor";
/// Contract id and major version for audio-editor sub-editor contributions.
pub const AUDIO_SUB_EDITOR_CONTRACT: &str = "notwig.audio_editor.sub_editor@1";
/// Maximum length of a sub-editor title.
pub const MAX_AUDIO_SUB_EDITOR_TITLE_BYTES: usize = 64;

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

/// Contribution body for one audio-editor sub-editor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioSubEditorKind {
    /// A shared curve editor over a host-owned clip curve.
    Curve(CurveTargetSpec),
}

impl AudioSubEditorKind {
    /// Returns the curve target when this is a shared curve sub-editor.
    #[must_use]
    pub const fn curve(&self) -> Option<&CurveTargetSpec> {
        match self {
            Self::Curve(spec) => Some(spec),
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
}
