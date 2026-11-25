use std::path::{Component, Path, PathBuf};

use notwig_ext_api::{ExtensionManifest, target};

use crate::error::ExtensionError;

const MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024;

/// An extension directory with a validated manifest and loadable entrypoint.
#[derive(Debug, Clone)]
pub struct DiscoveredExtension {
    /// Parsed and validated manifest.
    pub manifest: ExtensionManifest,
    /// Extension directory.
    pub dir: PathBuf,
    /// Absolute path to the selected entrypoint binary.
    pub entry_path: PathBuf,
    /// Canonical target of the selected entrypoint.
    pub target: String,
    /// Supported entrypoints in manifest order, including the selected one.
    pub candidates: Vec<(String, PathBuf)>,
}

/// One extension directory that could not be discovered.
#[derive(Debug, Clone)]
pub struct DiscoveryFailure {
    /// Manifest id when it was readable.
    pub id: Option<String>,
    /// Fallback directory name used for display.
    pub name: String,
    /// Directory that failed.
    pub path: PathBuf,
    /// Human-readable failure reason.
    pub error: String,
}

/// Result of scanning an extensions directory.
#[derive(Debug, Clone, Default)]
pub struct DiscoveryReport {
    /// Successfully discovered extensions.
    pub extensions: Vec<DiscoveredExtension>,
    /// Directories that failed validation or entrypoint selection.
    pub failures: Vec<DiscoveryFailure>,
}

/// Scans `root` for extension subdirectories.
///
/// A missing root yields an empty report. Per-extension problems are reported as
/// `DiscoveryFailure` entries rather than aborting the scan.
pub fn discover(root: &Path) -> Result<DiscoveryReport, ExtensionError> {
    let mut report = DiscoveryReport::default();
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(report),
        Err(error) => {
            return Err(ExtensionError::Io(format!(
                "read extensions directory {}: {error}",
                root.display()
            )));
        }
    };
    let mut dirs = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| ExtensionError::Io(error.to_string()))?;
        let path = entry.path();
        if path.is_dir() {
            dirs.push(path);
        }
    }
    dirs.sort();
    for dir in dirs {
        let fallback_name = dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("extension")
            .to_owned();
        match load_one(&dir) {
            Ok(extension) => report.extensions.push(extension),
            Err((id, error)) => report.failures.push(DiscoveryFailure {
                id,
                name: fallback_name,
                path: dir,
                error,
            }),
        }
    }
    Ok(report)
}

fn load_one(dir: &Path) -> Result<DiscoveredExtension, (Option<String>, String)> {
    let manifest_path = dir.join("extension.json");
    let text = std::fs::read_to_string(&manifest_path)
        .map_err(|error| (None, format!("read {}: {error}", manifest_path.display())))?;
    let manifest: ExtensionManifest = serde_json::from_str(&text)
        .map_err(|error| (None, format!("parse {}: {error}", manifest_path.display())))?;
    let id = Some(manifest.id.clone());
    manifest
        .validate()
        .map_err(|error| (id.clone(), error.to_string()))?;

    let mut candidates = Vec::new();
    for entry in manifest
        .entrypoints
        .iter()
        .filter(|entry| target::is_supported(&entry.target))
    {
        let relative = Path::new(&entry.entrypoint);
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            return Err((
                id,
                "extension entrypoint must stay inside the extension directory".into(),
            ));
        }
        let entry_path = dir.join(relative);
        let metadata = std::fs::metadata(&entry_path).map_err(|error| {
            (
                id.clone(),
                format!("read {}: {error}", entry_path.display()),
            )
        })?;
        if metadata.len() > MAX_ENTRY_BYTES {
            return Err((
                id.clone(),
                format!(
                    "extension entrypoint is {} bytes (limit {MAX_ENTRY_BYTES})",
                    metadata.len()
                ),
            ));
        }
        candidates.push((entry.target.clone(), entry_path));
    }
    let Some((target, entry_path)) = candidates.first().cloned() else {
        let declared = manifest
            .entrypoints
            .iter()
            .map(|entry| entry.target.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err((
            id,
            format!("no supported entrypoint for this host (declared: {declared})"),
        ));
    };
    Ok(DiscoveredExtension {
        manifest,
        dir: dir.to_path_buf(),
        entry_path,
        target,
        candidates,
    })
}
