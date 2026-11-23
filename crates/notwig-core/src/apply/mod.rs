mod assets;
mod audio;
mod clips;
mod devices;
mod dispatch;
mod notes;
mod tracks;
mod transport;

use crate::domain::ProjectDoc;
use crate::protocol::{EngineMsg, StateDelta};

pub(crate) use audio::audio_slice_window_set;
pub use dispatch::apply;
pub use tracks::create_track;

#[derive(Debug, Clone, PartialEq)]
pub struct ApplyResult {
    pub doc: ProjectDoc,
    pub delta: StateDelta,
    pub engine_msgs: Vec<EngineMsg>,
}
