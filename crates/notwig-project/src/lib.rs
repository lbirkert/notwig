#![forbid(unsafe_code)]

mod assets;
mod bundle;
mod error;
mod export;
mod fsutil;
mod manifest;
mod probe;
mod resample;

pub use assets::{
    ImportedAsset, decode_audio, decode_audio_bytes, import_asset, write_wav_f32,
    write_wav_f32_bytes,
};
pub use bundle::{
    BundleFiles, LoadResult, SaveResult, autosave, bundle_zip_bytes, cleanup_autosaves,
    create_session_lock, has_session_lock, list_autosaves, load_latest_autosave, open, open_files,
    remove_session_lock, save, save_files, session_lock_path,
};
pub use error::ProjectError;
pub use export::{ExportBitDepth, ExportFormat, ExportOptions, write_audio_export};
pub use manifest::{Manifest, ManifestIssue};
pub use probe::{AudioProbe, probe_audio};
pub use resample::{ResampleQuality, resample};
