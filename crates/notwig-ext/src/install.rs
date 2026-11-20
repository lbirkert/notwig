use std::collections::BTreeSet;
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use notwig_ext_api::ExtensionManifest;
use zip::ZipArchive;

use crate::error::ExtensionError;

const MAX_ARCHIVE_BYTES: usize = 64 * 1024 * 1024;
const MAX_UNCOMPRESSED_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 1024;
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct InstalledExtension {
    pub manifest: ExtensionManifest,
    pub path: PathBuf,
}

pub fn install_zip_file(
    root: &Path,
    path: &Path,
    expected_id: Option<&str>,
) -> Result<InstalledExtension, ExtensionError> {
    let bytes = std::fs::read(path)
        .map_err(|error| ExtensionError::Io(format!("read {}: {error}", path.display())))?;
    install_zip_bytes(root, &bytes, expected_id)
}

pub fn install_zip_bytes(
    root: &Path,
    bytes: &[u8],
    expected_id: Option<&str>,
) -> Result<InstalledExtension, ExtensionError> {
    if bytes.len() > MAX_ARCHIVE_BYTES {
        return Err(ExtensionError::ArchiveTooLarge);
    }
    std::fs::create_dir_all(root).map_err(|error| {
        ExtensionError::Io(format!(
            "create extensions directory {}: {error}",
            root.display()
        ))
    })?;
    let suffix = unique_suffix();
    let staging = root.join(format!(".install-{}-{suffix}", std::process::id()));
    if let Err(error) = std::fs::create_dir_all(&staging) {
        return Err(ExtensionError::Io(format!(
            "create staging directory {}: {error}",
            staging.display()
        )));
    }
    let result = extract_archive(bytes, &staging, expected_id);
    let manifest = match result {
        Ok(manifest) => manifest,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(error);
        }
    };
    let target = root.join(&manifest.id);
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
        manifest,
        path: target,
    })
}

pub fn uninstall(root: &Path, id: &str) -> Result<(), ExtensionError> {
    validate_id(id)?;
    let target = root.join(id);
    if target.exists() {
        std::fs::remove_dir_all(&target)
            .map_err(|error| ExtensionError::Io(format!("remove {}: {error}", target.display())))?;
    }
    Ok(())
}

fn extract_archive(
    bytes: &[u8],
    staging: &Path,
    expected_id: Option<&str>,
) -> Result<ExtensionManifest, ExtensionError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| ExtensionError::Archive(error.to_string()))?;
    if archive.len() > MAX_ENTRIES {
        return Err(ExtensionError::ArchiveTooLarge);
    }
    let (manifest_index, base) = find_manifest(&mut archive)?;
    let mut manifest_file = archive
        .by_index(manifest_index)
        .map_err(|error| ExtensionError::Archive(error.to_string()))?;
    if manifest_file.size() > MAX_MANIFEST_BYTES {
        return Err(ExtensionError::ArchiveTooLarge);
    }
    let mut manifest_bytes = Vec::new();
    manifest_file
        .read_to_end(&mut manifest_bytes)
        .map_err(|error| ExtensionError::Archive(error.to_string()))?;
    drop(manifest_file);
    let manifest: ExtensionManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| ExtensionError::Manifest(error.to_string()))?;
    manifest
        .validate()
        .map_err(|error| ExtensionError::Manifest(error.to_string()))?;
    if let Some(expected_id) = expected_id {
        if manifest.id != expected_id {
            return Err(ExtensionError::ExtensionIdMismatch {
                expected: expected_id.to_owned(),
                found: manifest.id.clone(),
            });
        }
    }
    validate_id(&manifest.id)?;

    let mut total = 0_u64;
    let mut extracted = BTreeSet::new();
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|error| ExtensionError::Archive(error.to_string()))?;
        let Some(path) = file.enclosed_name() else {
            return Err(ExtensionError::UnsafeArchivePath(file.name().to_owned()));
        };
        let Some(relative) = strip_base(&path, &base) else {
            continue;
        };
        if relative.as_os_str().is_empty() {
            continue;
        }
        if file
            .unix_mode()
            .is_some_and(|mode| mode & 0o170_000 == 0o120_000)
        {
            return Err(ExtensionError::UnsafeArchivePath(file.name().to_owned()));
        }
        if !valid_relative(&relative) {
            return Err(ExtensionError::UnsafeArchivePath(file.name().to_owned()));
        }
        let target = staging.join(&relative);
        if file.is_dir() {
            std::fs::create_dir_all(&target).map_err(|error| {
                ExtensionError::Io(format!("create {}: {error}", target.display()))
            })?;
            continue;
        }
        total = total.saturating_add(file.size());
        if total > MAX_UNCOMPRESSED_BYTES {
            return Err(ExtensionError::ArchiveTooLarge);
        }
        if !extracted.insert(relative.clone()) {
            return Err(ExtensionError::Archive(format!(
                "duplicate archive path: {}",
                relative.display()
            )));
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                ExtensionError::Io(format!("create {}: {error}", parent.display()))
            })?;
        }
        let mut data = Vec::with_capacity(usize::try_from(file.size()).unwrap_or(0));
        file.read_to_end(&mut data)
            .map_err(|error| ExtensionError::Archive(error.to_string()))?;
        std::fs::write(&target, data)
            .map_err(|error| ExtensionError::Io(format!("write {}: {error}", target.display())))?;
    }
    Ok(manifest)
}

fn find_manifest(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
) -> Result<(usize, PathBuf), ExtensionError> {
    let mut found: Option<(usize, PathBuf)> = None;
    for index in 0..archive.len() {
        let file = archive
            .by_index(index)
            .map_err(|error| ExtensionError::Archive(error.to_string()))?;
        let Some(path) = file.enclosed_name() else {
            return Err(ExtensionError::UnsafeArchivePath(file.name().to_owned()));
        };
        if path.file_name().is_none_or(|name| name != "extension.json") {
            continue;
        }
        let components: Vec<Component<'_>> = path.components().collect();
        if components.len() > 2 {
            continue;
        }
        let base = if components.len() == 2 {
            PathBuf::from(components[0].as_os_str())
        } else {
            PathBuf::new()
        };
        if found.is_some() {
            return Err(ExtensionError::Archive(
                "archive contains more than one extension.json".into(),
            ));
        }
        found = Some((index, base));
    }
    found.ok_or(ExtensionError::MissingManifest)
}

fn strip_base(path: &Path, base: &Path) -> Option<PathBuf> {
    if base.as_os_str().is_empty() {
        Some(path.to_path_buf())
    } else {
        path.strip_prefix(base).ok().map(Path::to_path_buf)
    }
}

fn valid_relative(path: &Path) -> bool {
    path.components()
        .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
        && !path.as_os_str().is_empty()
}

fn validate_id(id: &str) -> Result<(), ExtensionError> {
    if id.is_empty()
        || id == "."
        || id == ".."
        || id.contains('/')
        || id.contains('\\')
        || id.contains('\0')
    {
        return Err(ExtensionError::InvalidExtensionId(id.to_owned()));
    }
    Ok(())
}

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
}
