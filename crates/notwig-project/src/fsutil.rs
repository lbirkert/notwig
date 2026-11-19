use std::path::{Path, PathBuf};

use crate::error::ProjectError;

pub(crate) fn ensure_dir(path: &Path) -> Result<(), ProjectError> {
    std::fs::create_dir_all(path)?;
    Ok(())
}

pub(crate) fn write_atomic(path: &Path, contents: &[u8]) -> Result<(), ProjectError> {
    let temporary = temporary_path(path);
    if let Some(parent) = temporary.parent() {
        ensure_dir(parent)?;
    }
    std::fs::write(&temporary, contents)?;
    finalize_atomic(&temporary, path)
}

pub(crate) fn write_json_atomic<T>(path: &Path, value: &T) -> Result<(), ProjectError>
where
    T: serde::Serialize + ?Sized,
{
    let text = serde_json::to_string_pretty(value)?;
    write_atomic(path, text.as_bytes())
}

pub(crate) fn finalize_atomic(temporary: &Path, target: &Path) -> Result<(), ProjectError> {
    if let Some(parent) = target.parent() {
        ensure_dir(parent)?;
    }
    std::fs::rename(temporary, target)?;
    Ok(())
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".tmp");
    PathBuf::from(name)
}
