//! Extension source discovery and Cargo metadata.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use notwig_ext_api::manifest::ExtensionManifest;
use serde_json::Value;

use crate::error::{Error, Result};

pub(super) const MANIFEST_NAME: &str = "extension.json";
pub(super) const DEFAULT_ICON: &str = "icon.png";

pub(super) struct PackageMetadata {
    pub(super) target_stem: String,
    pub(super) target_directory: PathBuf,
}

pub(super) fn resolve_source_dir(explicit: Option<&Path>) -> Result<PathBuf> {
    let mut path = match explicit {
        Some(path) if path.is_file() => path
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| Error::message(format!("{} has no parent", path.display())))?,
        Some(path) => path.to_path_buf(),
        None => std::env::current_dir().map_err(|error| Error::io(".", error))?,
    };
    loop {
        if path.join(MANIFEST_NAME).is_file() {
            return Ok(path);
        }
        if !path.pop() {
            return Err(Error::message(format!(
                "could not find {MANIFEST_NAME} (run from an extension directory or pass --path)"
            )));
        }
    }
}

pub(super) fn read_manifest(source_dir: &Path) -> Result<ExtensionManifest> {
    let manifest_path = source_dir.join(MANIFEST_NAME);
    let text =
        fs::read_to_string(&manifest_path).map_err(|error| Error::io(&manifest_path, error))?;
    let manifest: ExtensionManifest = serde_json::from_str(&text)?;
    manifest
        .validate_source()
        .map_err(|error| Error::message(format!("{}: {error}", manifest_path.display())))?;
    Ok(manifest)
}

pub(super) fn package_metadata(source_dir: &Path) -> Result<PackageMetadata> {
    let manifest_path = source_dir.join("Cargo.toml");
    if !manifest_path.is_file() {
        return Err(Error::message(format!(
            "{}: extension source must contain a Cargo package",
            manifest_path.display()
        )));
    }
    let cargo = cargo_binary();
    let output = Command::new(cargo)
        .arg("metadata")
        .arg("--format-version")
        .arg("1")
        .arg("--no-deps")
        .arg("--manifest-path")
        .arg(&manifest_path)
        .output()
        .map_err(|error| Error::message(format!("run cargo metadata: {error}")))?;
    if !output.status.success() {
        return Err(Error::message(format!(
            "cargo metadata failed:\n{}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let value: Value = serde_json::from_slice(&output.stdout)?;
    let packages = value
        .get("packages")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::message("cargo metadata omitted packages"))?;
    let wanted = canonical(&manifest_path);
    let package = packages
        .iter()
        .find(|package| {
            package
                .get("manifest_path")
                .and_then(Value::as_str)
                .map(Path::new)
                .is_some_and(|path| canonical(path) == wanted)
        })
        .ok_or_else(|| {
            Error::message(format!(
                "cargo metadata did not describe {}",
                manifest_path.display()
            ))
        })?;
    let target_stem = package
        .get("targets")
        .and_then(Value::as_array)
        .and_then(|targets| {
            targets.iter().find_map(|target| {
                let kinds = target.get("kind")?.as_array()?;
                let is_library = kinds.iter().any(|kind| {
                    matches!(
                        kind.as_str(),
                        Some("lib" | "cdylib" | "staticlib" | "dylib")
                    )
                });
                is_library
                    .then(|| target.get("name")?.as_str().map(ToOwned::to_owned))
                    .flatten()
            })
        })
        .ok_or_else(|| Error::message("extension package has no library target"))?;
    let target_directory = value
        .get("target_directory")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| Error::message("cargo metadata omitted target_directory"))?;
    Ok(PackageMetadata {
        target_stem,
        target_directory,
    })
}

fn canonical(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub(super) fn cargo_binary() -> std::ffi::OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
}
