#![forbid(unsafe_code)]

mod assets;
mod bundle;
mod error;
mod fsutil;
mod manifest;

pub use assets::{
    ImportedAsset, ResampleQuality, decode_audio, import_asset, resample, write_wav_f32,
};
pub use bundle::{
    LoadResult, SaveResult, autosave, cleanup_autosaves, create_session_lock, has_session_lock,
    list_autosaves, load_latest_autosave, open, remove_session_lock, save, session_lock_path,
};
pub use error::ProjectError;
pub use manifest::{Manifest, ManifestIssue};
