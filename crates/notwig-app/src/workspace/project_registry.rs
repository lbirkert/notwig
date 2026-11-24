use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const REGISTRY_VERSION: u32 = 1;
const MAX_ENTRIES: usize = 64;

/// A project bundle that has an unclean session lock and autosave snapshots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryCandidate {
    pub bundle: PathBuf,
    pub project_name: String,
    pub autosave_count: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedRegistry {
    #[serde(default = "default_version")]
    version: u32,
    #[serde(default)]
    bundles: Vec<PathBuf>,
}

impl Default for PersistedRegistry {
    fn default() -> Self {
        Self {
            version: REGISTRY_VERSION,
            bundles: Vec::new(),
        }
    }
}

const fn default_version() -> u32 {
    REGISTRY_VERSION
}

pub fn register_bundle(bundle: &Path) -> Result<(), String> {
    let Some(path) = config_path() else {
        return Ok(());
    };
    register_bundle_at(&path, bundle)
}

pub fn registered_bundles() -> Vec<PathBuf> {
    config_path().map_or_else(Vec::new, |path| registered_bundles_at(&path))
}

pub fn recovery_candidates() -> Vec<RecoveryCandidate> {
    recovery_candidates_in(&registered_bundles())
}

/// Register an existing bundle in the registry at `registry_file`.
#[doc(hidden)]
pub fn register_bundle_at(registry_file: &Path, bundle: &Path) -> Result<(), String> {
    let mut registry = read_registry(registry_file);
    let normalized = normalize_bundle(bundle);
    registry
        .bundles
        .retain(|entry| entry != &normalized && is_bundle(entry));
    registry.bundles.push(normalized);
    if registry.bundles.len() > MAX_ENTRIES {
        let overflow = registry.bundles.len() - MAX_ENTRIES;
        registry.bundles.drain(0..overflow);
    }
    write_registry(registry_file, &registry)
}

#[doc(hidden)]
pub fn registered_bundles_at(registry_file: &Path) -> Vec<PathBuf> {
    read_registry(registry_file)
        .bundles
        .into_iter()
        .filter(|bundle| is_bundle(bundle))
        .collect()
}

#[doc(hidden)]
pub fn recovery_candidates_in(bundles: &[PathBuf]) -> Vec<RecoveryCandidate> {
    let mut seen = BTreeSet::new();
    let mut candidates = Vec::new();
    for bundle in bundles {
        let normalized = normalize_bundle(bundle);
        if !seen.insert(normalized.clone()) || !is_bundle(&normalized) {
            continue;
        }
        if !notwig_project::has_session_lock(&normalized) {
            continue;
        }
        let autosaves = notwig_project::list_autosaves(&normalized);
        if autosaves.is_empty() {
            continue;
        }
        let project_name = normalized
            .file_stem()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("Project")
            .to_owned();
        candidates.push(RecoveryCandidate {
            bundle: normalized,
            project_name,
            autosave_count: autosaves.len(),
        });
    }
    candidates.sort_by(|a, b| a.bundle.cmp(&b.bundle));
    candidates
}

fn config_path() -> Option<PathBuf> {
    crate::app_paths::data_dir().map(|dir| dir.join("projects.json"))
}

fn read_registry(path: &Path) -> PersistedRegistry {
    let Ok(text) = std::fs::read_to_string(path) else {
        return PersistedRegistry::default();
    };
    match serde_json::from_str::<PersistedRegistry>(&text) {
        Ok(registry) => registry,
        Err(err) => {
            tracing::warn!(path = %path.display(), error = %err, "ignoring invalid project registry");
            PersistedRegistry::default()
        }
    }
}

fn write_registry(path: &Path, registry: &PersistedRegistry) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("create project registry dir: {err}"))?;
    }
    let text = serde_json::to_string_pretty(registry)
        .map_err(|err| format!("encode project registry: {err}"))?;
    std::fs::write(path, text).map_err(|err| format!("save project registry: {err}"))
}

fn normalize_bundle(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn is_bundle(path: &Path) -> bool {
    path.is_dir() && path.join("project.json").is_file()
}
