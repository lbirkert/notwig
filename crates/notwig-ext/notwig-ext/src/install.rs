use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use notwig_ext_api::ExtensionManifest;

use crate::archive::{MAX_ARCHIVE_BYTES, read_archive, validate_extension_id};
use crate::error::ExtensionError;

/// An extension installed from a ZIP package.
#[derive(Debug, Clone)]
pub struct InstalledExtension {
    /// Manifest read from the package.
    pub manifest: ExtensionManifest,
    /// Install directory (`root/id`).
    pub path: PathBuf,
}

/// Installs a ZIP file from disk under `root`.
///
/// When `expected_id` is set, the package id must match. Existing installs are
/// replaced through a staging directory and backup.
pub fn install_zip_file(
    root: &Path,
    path: &Path,
    expected_id: Option<&str>,
) -> Result<InstalledExtension, ExtensionError> {
    let bytes = std::fs::read(path)
        .map_err(|error| ExtensionError::Io(format!("read {}: {error}", path.display())))?;
    install_zip_bytes(root, &bytes, expected_id)
}

/// Installs a ZIP package from memory under `root`.
///
/// Validates the archive and manifest before writing, stages files in a hidden
/// directory, and atomically renames them into place. When `expected_id` is set,
/// the package id must match.
pub fn install_zip_bytes(
    root: &Path,
    bytes: &[u8],
    expected_id: Option<&str>,
) -> Result<InstalledExtension, ExtensionError> {
    if bytes.len() > MAX_ARCHIVE_BYTES {
        return Err(ExtensionError::ArchiveTooLarge);
    }
    let archive = read_archive(bytes, expected_id)?;
    std::fs::create_dir_all(root).map_err(|error| {
        ExtensionError::Io(format!(
            "create extensions directory {}: {error}",
            root.display()
        ))
    })?;
    let suffix = unique_suffix();
    let staging = root.join(format!(".install-{}-{suffix}", std::process::id()));
    if let Err(error) = write_files(&staging, &archive.files) {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(error);
    }
    let target = root.join(&archive.manifest.id);
    let backup = root.join(format!(".backup-{}-{suffix}", std::process::id()));
    let had_target = target.exists();
    if had_target {
        std::fs::rename(&target, &backup).map_err(|error| {
            let _ = std::fs::remove_dir_all(&staging);
            ExtensionError::Io(format!("backup {}: {error}", target.display()))
        })?;
    }
    if let Err(error) = std::fs::rename(&staging, &target) {
        let _ = std::fs::remove_dir_all(&staging);
        if had_target {
            let _ = std::fs::rename(&backup, &target);
        }
        return Err(ExtensionError::Io(format!(
            "install {}: {error}",
            target.display()
        )));
    }
    if had_target {
        let _ = std::fs::remove_dir_all(&backup);
    }
    Ok(InstalledExtension {
        manifest: archive.manifest,
        path: target,
    })
}

/// Removes the installed extension directory for `id`, if present.
///
/// Validates the id before deleting so it cannot escape `root`.
pub fn uninstall(root: &Path, id: &str) -> Result<(), ExtensionError> {
    validate_extension_id(id)?;
    let target = root.join(id);
    if target.exists() {
        std::fs::remove_dir_all(&target)
            .map_err(|error| ExtensionError::Io(format!("remove {}: {error}", target.display())))?;
    }
    Ok(())
}

fn write_files(
    staging: &Path,
    files: &std::collections::BTreeMap<PathBuf, Vec<u8>>,
) -> Result<(), ExtensionError> {
    std::fs::create_dir_all(staging).map_err(|error| {
        ExtensionError::Io(format!(
            "create staging directory {}: {error}",
            staging.display()
        ))
    })?;
    for (relative, data) in files {
        let target = staging.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                ExtensionError::Io(format!("create {}: {error}", parent.display()))
            })?;
        }
        std::fs::write(&target, data)
            .map_err(|error| ExtensionError::Io(format!("write {}: {error}", target.display())))?;
    }
    Ok(())
}

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
}
