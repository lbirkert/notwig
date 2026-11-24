use notwig_core::TrackId;
use notwig_core::edit::{ClipContent, Extract};
use notwig_core::sequencer::AudioSlice;

use crate::views::audio_editor::ClipLane;

#[derive(Debug, Clone, Default, PartialEq)]
pub enum Clipboard {
    #[default]
    Empty,
    Arranger(Extract<TrackId, ClipContent>),
    AudioClip(Extract<ClipLane, AudioSlice>),
    Devices,
}

impl Clipboard {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }
}
