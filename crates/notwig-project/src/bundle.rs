use std::path::{Path, PathBuf};

use notwig_core::{ProjectDoc, SCHEMA_VERSION};

use crate::error::ProjectError;
use crate::manifest::{Manifest, ManifestIssue};

#[derive(Debug, Clone)]
pub struct LoadResult {
    pub doc: ProjectDoc,
    pub issues: Vec<ManifestIssue>,
    pub recovery_offered: bool,
}

#[derive(Debug, Clone)]
pub struct SaveResult {
    pub path: PathBuf,
    pub revision: u64,
}

pub fn save(doc: &ProjectDoc, bundle: &Path) -> Result<SaveResult, ProjectError> {
    doc.validate()
        .map_err(|err| ProjectError::Invalid(err.to_string()))?;
    std::fs::create_dir_all(bundle)?;
    for dir in ["assets", "state", "autosave", "peaks", "cache"] {
        std::fs::create_dir_all(bundle.join(dir))?;
    }
    let manifest = Manifest::new(doc);
    let value = serde_json::to_value(&manifest)?;
    let text = serde_json::to_string_pretty(&value)?;
    let target = bundle.join("project.json");
    let temporary = bundle.join("project.json.tmp");
    std::fs::write(&temporary, text)?;
    std::fs::rename(&temporary, &target)?;
    Ok(SaveResult {
        path: target,
        revision: doc.revision,
    })
}

pub fn open(bundle: &Path) -> Result<LoadResult, ProjectError> {
    let path = bundle.join("project.json");
    let text = std::fs::read_to_string(&path).map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            ProjectError::Invalid(format!("missing project manifest at {}", path.display()))
        } else {
            ProjectError::Io(err.to_string())
        }
    })?;
    let value: serde_json::Value = serde_json::from_str(&text)?;
    let mut doc = if value.get("doc").is_some() {
        let manifest: Manifest = serde_json::from_value(value)?;
        check_schema(manifest.schema)?;
        manifest.doc
    } else {
        let doc: ProjectDoc = serde_json::from_value(value)?;
        check_schema(doc.schema)?;
        doc
    };
    let mut issues = Vec::new();
    normalize_loaded(&mut doc);
    if let Err(err) = doc.validate() {
        issues.push(ManifestIssue {
            entity: "document".into(),
            message: err.to_string(),
        });
    }
    for asset in doc.assets.assets.values() {
        let asset_path = bundle.join(&asset.uri);
        if !asset_path.exists() {
            issues.push(ManifestIssue {
                entity: asset.id.to_string(),
                message: format!("missing asset file {}", asset_path.display()),
            });
        }
    }
    let recovery_offered =
        bundle.join("session.lock").exists() && !list_autosaves(bundle).is_empty();
    Ok(LoadResult {
        doc,
        issues,
        recovery_offered,
    })
}

fn check_schema(schema: notwig_core::SchemaVersion) -> Result<(), ProjectError> {
    if schema.major != SCHEMA_VERSION.major {
        return Err(ProjectError::UnsupportedVersion(format!(
            "{} (supported major {})",
            schema.major, SCHEMA_VERSION.major
        )));
    }
    Ok(())
}

fn normalize_loaded(doc: &mut ProjectDoc) {
    doc.transport.playing = false;
    doc.transport.recording = false;
}

pub fn autosave(doc: &ProjectDoc, bundle: &Path, timestamp: i64) -> Result<PathBuf, ProjectError> {
    let dir = bundle.join("autosave");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("snapshot-{}-{timestamp}.json", doc.revision));
    let text = serde_json::to_string_pretty(&serde_json::to_value(doc)?)?;
    std::fs::write(&path, text)?;
    Ok(path)
}

pub fn list_autosaves(bundle: &Path) -> Vec<PathBuf> {
    let dir = bundle.join("autosave");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    paths.sort();
    paths
}

pub fn load_latest_autosave(bundle: &Path) -> Result<Option<ProjectDoc>, ProjectError> {
    let Some(path) = list_autosaves(bundle).pop() else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(path)?;
    let mut doc: ProjectDoc = serde_json::from_str(&text)?;
    check_schema(doc.schema)?;
    normalize_loaded(&mut doc);
    doc.validate()
        .map_err(|err| ProjectError::Invalid(err.to_string()))?;
    Ok(Some(doc))
}

pub fn cleanup_autosaves(bundle: &Path) -> Result<(), ProjectError> {
    for path in list_autosaves(bundle) {
        let _ = std::fs::remove_file(path);
    }
    Ok(())
}
