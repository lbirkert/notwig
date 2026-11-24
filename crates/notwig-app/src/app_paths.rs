//! Canonical per-user application directories.
//!
//! Preferences, extensions, project history, and logs live under one app
//! directory. The plugin scan cache is disposable and therefore stays in the
//! platform cache directory.

use std::path::PathBuf;

pub(crate) fn data_dir() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    #[cfg(target_os = "macos")]
    let path = home.join("Library/Application Support/notwig");
    #[cfg(not(target_os = "macos"))]
    let path = home.join(".config/notwig");
    Some(path)
}

pub(crate) fn cache_dir() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    #[cfg(target_os = "macos")]
    let path = home.join("Library/Caches/notwig");
    #[cfg(not(target_os = "macos"))]
    let path = home.join(".cache/notwig");
    Some(path)
}

pub(crate) fn log_dir() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join("logs"))
}
