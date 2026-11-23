//! ZIP packaging of built entrypoints, icons, and declared includes.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use notwig_ext_api::manifest::{ExtensionEntryPoint, ExtensionInclude, ExtensionManifest};
use notwig_ext_api::target;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::error::{Error, Result};
use crate::target::BuildTarget;

use super::compile::{artifact_path, resolve_target};
use super::paths::{ensure_zip_extension, safe_relative, sanitized_id};
use super::source::{
    DEFAULT_ICON, MANIFEST_NAME, package_metadata, read_manifest, resolve_source_dir,
};

/// Packages whichever target binaries already exist in Cargo's target directory.
pub fn package_targets(
    target_names: &[String],
    path: Option<&Path>,
    out_dir: &Path,
    name: Option<&str>,
) -> Result<std::path::PathBuf> {
    let source_dir = resolve_source_dir(path)?;
    let manifest = read_manifest(&source_dir)?;
    let package = package_metadata(&source_dir)?;
    let selected = resolve_targets(target_names)?;
    let explicit = !target_names.is_empty();

    let mut entries = Vec::new();
    let mut missing = Vec::new();
    for target in selected {
        let artifact = artifact_path(&package, target);
        if artifact.is_file() {
            let bytes = fs::read(&artifact).map_err(|error| Error::io(&artifact, error))?;
            entries.push(PackageEntry {
                target,
                entrypoint: target.entrypoint_name(&package.target_stem),
                bytes,
            });
        } else {
            missing.push(artifact);
        }
    }
    if entries.is_empty() {
        return Err(Error::message(format!(
            "no entrypoint binaries found in {}.\nRun `notwig-ext-cli build --entrypoint <target>` on each target platform and copy the artifacts into the same target directory before packaging.",
            package.target_directory.display()
        )));
    }
    if explicit && !missing.is_empty() {
        return Err(Error::message(format!(
            "missing entrypoint binaries for explicitly selected targets:\n  {}",
            missing
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join("\n  ")
        )));
    }

    if out_dir.as_os_str().is_empty() {
        return Err(Error::message("--out-dir must not be empty"));
    }
    fs::create_dir_all(out_dir).map_err(|error| Error::io(out_dir, error))?;
    let file_name = name.map_or_else(
        || {
            if entries.len() == 1 {
                default_zip_name(&manifest, entries[0].target)
            } else {
                format!("{}.zip", sanitized_id(&manifest))
            }
        },
        ToOwned::to_owned,
    );
    let file_name = ensure_zip_extension(file_name)?;
    let out_path = out_dir.join(file_name);
    package_entries(&manifest, &source_dir, &entries, &out_path)?;
    println!("packaged {}", out_path.display());
    Ok(out_path)
}

fn resolve_targets(target_names: &[String]) -> Result<Vec<BuildTarget>> {
    if target_names.is_empty() {
        return Ok(crate::target::all().to_vec());
    }
    let mut targets = Vec::new();
    for raw in target_names {
        let target = resolve_target(raw)?;
        if !targets.contains(&target) {
            targets.push(target);
        }
    }
    Ok(targets)
}

fn default_zip_name(manifest: &ExtensionManifest, target: BuildTarget) -> String {
    format!("{}-{}.zip", sanitized_id(manifest), target.canonical)
}

/// One target entrypoint staged for packaging.
#[derive(Debug, Clone)]
pub struct PackageEntry {
    /// Build target of the entrypoint.
    pub target: BuildTarget,
    /// Relative entrypoint file name inside the package.
    pub entrypoint: String,
    /// Entrypoint binary contents.
    pub bytes: Vec<u8>,
}

/// Writes a ZIP package from prepared entrypoint and include files.
///
/// Rewrites the manifest entrypoints from `entries`, bundles the default icon
/// and declared includes, and rejects missing or conflicting files.
pub fn package_entries(
    manifest: &ExtensionManifest,
    source_dir: &Path,
    entries: &[PackageEntry],
    out_path: &Path,
) -> Result<()> {
    if entries.is_empty() {
        return Err(Error::message(
            "cannot package an extension with no entrypoints",
        ));
    }
    let includes = manifest.includes.clone();
    let mut manifest = manifest.clone();
    manifest.includes.clear();
    manifest.entrypoints = entries
        .iter()
        .map(|entry| ExtensionEntryPoint {
            entrypoint: entry.entrypoint.clone(),
            target: entry.target.canonical.to_owned(),
        })
        .collect();

    let mut files = BTreeMap::new();
    for entry in entries {
        files.insert(entry.entrypoint.clone(), entry.bytes.clone());
    }

    let default_icon = source_dir.join(DEFAULT_ICON);
    if default_icon.is_file() {
        let bytes = fs::read(&default_icon).map_err(|error| Error::io(&default_icon, error))?;
        files.insert(DEFAULT_ICON.to_owned(), bytes);
    }
    if manifest.image.is_none() && files.contains_key(DEFAULT_ICON) {
        manifest.image = Some(DEFAULT_ICON.to_owned());
    }
    if let Some(image) = &manifest.image {
        let path = safe_relative(image)?;
        let source = source_dir.join(&path);
        let bytes = fs::read(&source).map_err(|error| Error::io(&source, error))?;
        files.insert(path, bytes);
    }

    for include in includes {
        if !include_applies_any(&include, entries) {
            continue;
        }
        let path = safe_relative(&include.path)?;
        if path == MANIFEST_NAME || entries.iter().any(|entry| entry.entrypoint == path) {
            return Err(Error::message(format!(
                "include path conflicts with generated package file: {path}"
            )));
        }
        let source = source_dir.join(&path);
        let bytes = fs::read(&source).map_err(|error| Error::io(&source, error))?;
        match files.get(&path) {
            Some(existing) if existing == &bytes => {}
            Some(_) => {
                return Err(Error::message(format!(
                    "include path conflicts with another package file: {path}"
                )));
            }
            None => {
                files.insert(path, bytes);
            }
        }
    }

    let mut json = serde_json::to_vec_pretty(&manifest)?;
    json.push(b'\n');
    files.insert(MANIFEST_NAME.to_owned(), json);

    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent).map_err(|error| Error::io(parent, error))?;
    }
    let file = File::create(out_path).map_err(|error| Error::io(out_path, error))?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);
    for (name, bytes) in files {
        writer.start_file(name, options)?;
        writer
            .write_all(&bytes)
            .map_err(|error| Error::io(out_path, error))?;
    }
    writer.finish()?;
    Ok(())
}

/// Writes a single-target package from an already built artifact.
pub fn package_extension(
    manifest: &ExtensionManifest,
    source_dir: &Path,
    target: BuildTarget,
    entrypoint: &str,
    artifact: &[u8],
    out_path: &Path,
) -> Result<()> {
    package_entries(
        manifest,
        source_dir,
        &[PackageEntry {
            target,
            entrypoint: entrypoint.to_owned(),
            bytes: artifact.to_vec(),
        }],
        out_path,
    )
}

fn include_applies_any(include: &ExtensionInclude, entries: &[PackageEntry]) -> bool {
    include.target.as_ref().is_none_or(|wanted| {
        entries
            .iter()
            .any(|entry| target::canonical(wanted) == Some(entry.target.canonical))
    })
}
