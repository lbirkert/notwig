use std::path::{Component, Path, PathBuf};

use notwig_ext_api::ExtensionManifest;

use crate::error::ExtensionError;

const MAX_MODULE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct DiscoveredExtension {
    pub manifest: ExtensionManifest,
    pub dir: PathBuf,
    pub wasm_path: PathBuf,
    pub wasm: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct DiscoveryFailure {
    pub id: Option<String>,
    pub name: String,
    pub path: PathBuf,
    pub error: String,
}

#[derive(Debug, Clone, Default)]
pub struct DiscoveryReport {
    pub extensions: Vec<DiscoveredExtension>,
    pub failures: Vec<DiscoveryFailure>,
}

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

    let entry = Path::new(&manifest.entry);
    if entry.is_absolute()
        || entry
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err((
            id,
            "extension entry must stay inside the extension directory".into(),
        ));
    }
    let wasm_path = dir.join(entry);
    let metadata = std::fs::metadata(&wasm_path)
        .map_err(|error| (id.clone(), format!("read {}: {error}", wasm_path.display())))?;
    if metadata.len() > MAX_MODULE_BYTES {
        return Err((
            id,
            format!(
                "wasm module is {} bytes (limit {MAX_MODULE_BYTES})",
                metadata.len()
            ),
        ));
    }
    let wasm = std::fs::read(&wasm_path)
        .map_err(|error| (id.clone(), format!("read {}: {error}", wasm_path.display())))?;
    if !wasm.starts_with(b"\0asm") {
        return Err((id, "entry is not a wasm module".into()));
    }
    Ok(DiscoveredExtension {
        manifest,
        dir: dir.to_path_buf(),
        wasm_path,
        wasm,
    })
}
