#![forbid(unsafe_code)]

mod assets;
mod bundle;
mod error;
mod manifest;

pub use assets::{
    ImportedAsset, ResampleQuality, decode_audio, import_asset, resample, write_wav_f32,
};
pub use bundle::{
    LoadResult, SaveResult, autosave, cleanup_autosaves, list_autosaves, load_latest_autosave,
    open, save,
};
pub use error::ProjectError;
pub use manifest::{Manifest, ManifestIssue};
