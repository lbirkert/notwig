use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};

use notwig_ext_api::ExtensionManifest;
use zip::ZipArchive;

use crate::error::ExtensionError;

/// Maximum accepted ZIP size in bytes.
pub const MAX_ARCHIVE_BYTES: usize = 64 * 1024 * 1024;
const MAX_UNCOMPRESSED_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 1024;
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

/// A validated extension manifest and its extracted, relative file tree.
#[derive(Debug, Clone)]
pub struct ExtensionArchive {
    pub manifest: ExtensionManifest,
    pub files: BTreeMap<PathBuf, Vec<u8>>,
}

/// Reads and validates a ZIP archive, returning its files relative to the
/// extension root.
pub fn read_archive(
    bytes: &[u8],
    expected_id: Option<&str>,
) -> Result<ExtensionArchive, ExtensionError> {
    if bytes.len() > MAX_ARCHIVE_BYTES {
        return Err(ExtensionError::ArchiveTooLarge);
    }
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| ExtensionError::Archive(error.to_string()))?;
    if archive.len() > MAX_ENTRIES {
        return Err(ExtensionError::ArchiveTooLarge);
    }
    let mut files = BTreeMap::new();
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|error| ExtensionError::Archive(error.to_string()))?;
        let Some(path) = file.enclosed_name() else {
            return Err(ExtensionError::UnsafeArchivePath(file.name().to_owned()));
        };
        if file
            .unix_mode()
            .is_some_and(|mode| mode & 0o170_000 == 0o120_000)
        {
            return Err(ExtensionError::UnsafeArchivePath(file.name().to_owned()));
        }
        if file.is_dir() {
            continue;
        }
        total = total.saturating_add(file.size());
        if total > MAX_UNCOMPRESSED_BYTES {
            return Err(ExtensionError::ArchiveTooLarge);
        }
        if files.contains_key(&path) {
            return Err(ExtensionError::Archive(format!(
                "duplicate archive path: {}",
                path.display()
            )));
        }
        let mut data = Vec::with_capacity(usize::try_from(file.size()).unwrap_or(0));
        file.read_to_end(&mut data)
            .map_err(|error| ExtensionError::Archive(error.to_string()))?;
        files.insert(path, data);
    }
    read_extension_files(files, expected_id)
}

/// Validates a file tree read from an installed extension directory.
pub fn read_extension_files(
    files: BTreeMap<PathBuf, Vec<u8>>,
    expected_id: Option<&str>,
) -> Result<ExtensionArchive, ExtensionError> {
    let (manifest_path, base) = find_manifest(&files)?;
    let manifest_bytes = files
        .get(&manifest_path)
        .ok_or(ExtensionError::MissingManifest)?;
    if manifest_bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(ExtensionError::ArchiveTooLarge);
    }
    let manifest: ExtensionManifest = serde_json::from_slice(manifest_bytes)
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
    validate_extension_id(&manifest.id)?;

    let mut extracted = BTreeMap::new();
    let mut total = 0_u64;
    for (path, data) in files {
        let Some(relative) = strip_base(&path, &base) else {
            continue;
        };
        if relative.as_os_str().is_empty() {
            continue;
        }
        if !valid_relative(&relative) {
            return Err(ExtensionError::UnsafeArchivePath(
                path.display().to_string(),
            ));
        }
        total = total.saturating_add(data.len() as u64);
        if total > MAX_UNCOMPRESSED_BYTES {
            return Err(ExtensionError::ArchiveTooLarge);
        }
        if extracted.insert(relative.clone(), data).is_some() {
            return Err(ExtensionError::Archive(format!(
                "duplicate archive path: {}",
                relative.display()
            )));
        }
    }
    Ok(ExtensionArchive {
        manifest,
        files: extracted,
    })
}

/// Validates an extension id for use as an install directory name.
///
/// Rejects empty ids, `.`/`..`, and ids containing path separators or NUL.
pub fn validate_extension_id(id: &str) -> Result<(), ExtensionError> {
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

fn find_manifest(files: &BTreeMap<PathBuf, Vec<u8>>) -> Result<(PathBuf, PathBuf), ExtensionError> {
    let mut found: Option<(PathBuf, PathBuf)> = None;
    for path in files.keys() {
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
        found = Some((path.clone(), base));
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
