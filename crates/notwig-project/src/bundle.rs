use std::collections::BTreeMap;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use notwig_core::{CoreError, ProjectDoc, SCHEMA_VERSION};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::error::ProjectError;
use crate::fsutil;
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

/// A project bundle represented as relative-path to byte-content files.
///
/// Browser builds use this in place of direct filesystem access.
#[derive(Debug, Clone, Default)]
pub struct BundleFiles {
    pub files: BTreeMap<String, Vec<u8>>,
}

impl BundleFiles {
    pub fn insert(&mut self, path: impl Into<String>, bytes: impl Into<Vec<u8>>) {
        self.files.insert(path.into(), bytes.into());
    }

    #[must_use]
    pub fn get(&self, path: &str) -> Option<&[u8]> {
        self.files.get(path).map(Vec::as_slice)
    }
}

pub fn open_files(files: &BundleFiles) -> Result<LoadResult, ProjectError> {
    let text = files
        .get("project.json")
        .ok_or_else(|| ProjectError::Invalid("missing project manifest at project.json".into()))?;
    let value: serde_json::Value = serde_json::from_slice(text)?;
    let mut doc = if value.get("doc").is_some() {
        let manifest: Manifest = serde_json::from_value(value)?;
        validate_manifest_schema(&manifest)?;
        manifest.doc
    } else {
        parse_doc_value(value)?
    };
    let mut issues = Vec::new();
    if let Err(err) = normalize_and_validate(&mut doc) {
        issues.push(ManifestIssue {
            entity: "document".into(),
            message: err.to_string(),
        });
    }
    for asset in doc.assets.assets.values() {
        if !files.files.contains_key(&asset.uri) {
            issues.push(ManifestIssue {
                entity: asset.id.to_string(),
                message: format!("missing asset file {}", asset.uri),
            });
        }
    }
    Ok(LoadResult {
        doc,
        issues,
        recovery_offered: false,
    })
}

pub fn save_files(doc: &ProjectDoc, mut files: BundleFiles) -> Result<BundleFiles, ProjectError> {
    doc.validate()
        .map_err(|err| ProjectError::Invalid(err.to_string()))?;
    let manifest = Manifest::new(doc);
    let text = serde_json::to_vec_pretty(&manifest)?;
    files.insert("project.json", text);
    Ok(files)
}

/// Encodes a bundle as a ZIP archive suitable for browser download.
///
/// Entries are stored uncompressed so the encoder has no native compressor
/// dependency and works on every browser target.
pub fn bundle_zip_bytes(files: &BundleFiles) -> Result<Vec<u8>, ProjectError> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for (path, bytes) in &files.files {
        writer.start_file(path, options)?;
        writer.write_all(bytes)?;
    }
    Ok(writer.finish()?.into_inner())
}

pub fn save(doc: &ProjectDoc, bundle: &Path) -> Result<SaveResult, ProjectError> {
    doc.validate()
        .map_err(|err| ProjectError::Invalid(err.to_string()))?;
    fsutil::ensure_dir(bundle)?;
    for dir in ["assets", "state", "autosave", "peaks", "cache"] {
        fsutil::ensure_dir(&bundle.join(dir))?;
    }
    let manifest = Manifest::new(doc);
    let target = bundle.join("project.json");
    fsutil::write_json_atomic(&target, &manifest)?;
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
        validate_manifest_schema(&manifest)?;
        manifest.doc
    } else {
        parse_doc_value(value)?
    };
    let mut issues = Vec::new();
    if let Err(err) = normalize_and_validate(&mut doc) {
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

fn parse_doc_value(value: serde_json::Value) -> Result<ProjectDoc, ProjectError> {
    let doc: ProjectDoc = serde_json::from_value(value)?;
    check_schema(doc.schema)?;
    Ok(doc)
}

fn validate_manifest_schema(manifest: &Manifest) -> Result<(), ProjectError> {
    check_schema(manifest.schema)?;
    check_schema(manifest.doc.schema)?;
    if manifest.schema != manifest.doc.schema {
        return Err(ProjectError::Schema(format!(
            "manifest schema {:?} does not match document schema {:?}",
            manifest.schema, manifest.doc.schema
        )));
    }
    Ok(())
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

fn normalize_and_validate(doc: &mut ProjectDoc) -> Result<(), CoreError> {
    normalize_loaded(doc);
    doc.validate()
}

fn normalize_loaded(doc: &mut ProjectDoc) {
    doc.transport.playing = false;
    doc.transport.recording = false;
}

pub fn session_lock_path(bundle: &Path) -> PathBuf {
    bundle.join("session.lock")
}

pub fn has_session_lock(bundle: &Path) -> bool {
    session_lock_path(bundle).exists()
}

pub fn create_session_lock(bundle: &Path) -> Result<PathBuf, ProjectError> {
    std::fs::create_dir_all(bundle)?;
    let path = session_lock_path(bundle);
    let contents = format!("pid={}\n", std::process::id());
    std::fs::write(&path, contents)?;
    Ok(path)
}

pub fn remove_session_lock(bundle: &Path) -> Result<(), ProjectError> {
    let path = session_lock_path(bundle);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

pub fn autosave(doc: &ProjectDoc, bundle: &Path, timestamp: i64) -> Result<PathBuf, ProjectError> {
    let dir = bundle.join("autosave");
    let path = dir.join(format!("snapshot-{}-{timestamp}.json", doc.revision));
    fsutil::write_json_atomic(&path, doc)?;
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
    let value: serde_json::Value = serde_json::from_str(&text)?;
    let mut doc = parse_doc_value(value)?;
    normalize_and_validate(&mut doc).map_err(|err| ProjectError::Invalid(err.to_string()))?;
    Ok(Some(doc))
}

pub fn cleanup_autosaves(bundle: &Path) -> Result<(), ProjectError> {
    for path in list_autosaves(bundle) {
        let _ = std::fs::remove_file(path);
    }
    Ok(())
}
