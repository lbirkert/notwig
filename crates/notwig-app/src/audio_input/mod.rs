use notwig_core::{AudioInputRef, TrackId};

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use native::{InputHub, available_input_options};
#[cfg(target_arch = "wasm32")]
pub(crate) use web::{InputHub, available_input_options};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InputRequest {
    pub track_id: TrackId,
    pub input: AudioInputRef,
}
