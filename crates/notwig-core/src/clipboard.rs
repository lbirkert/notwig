//! Shared clipboard payloads.

use crate::edit::{ClipContent, Extract};
use crate::sequencer::AudioSlice;
use crate::{CurvePoint, TrackId};

/// Lane identity used by the audio clip editor's edit algebra.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClipLane;

/// Clipboard payload shared by host views and extension clipboard providers.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Clipboard {
    /// No clipboard contents.
    #[default]
    Empty,
    /// Arranger clip/event extract.
    Arranger(Extract<TrackId, ClipContent>),
    /// Audio clip slice extract.
    AudioClip(Extract<ClipLane, AudioSlice>),
    /// Curve points.
    Curve(Vec<CurvePoint>),
    /// Device snapshots.
    Devices,
}

impl Clipboard {
    /// Returns whether this clipboard has no contents.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }
}
