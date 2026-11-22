use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use notwig_ext_api::manifest::{ExtensionEntryPoint, ExtensionInclude, ExtensionManifest};
use notwig_ext_api::target;
use serde_json::Value;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::error::{Error, Result};
use crate::target::{self as build_target, BuildTarget};

const MANIFEST_NAME: &str = "extension.json";
const DEFAULT_ICON: &str = "icon.png";

/// Builds one target binary and leaves it in Cargo's target directory.
pub fn build_entrypoint(
    target_name: &str,
    path: Option<&Path>,
    install_targets: bool,
) -> Result<PathBuf> {
    let target = build_target::resolve(target_name).ok_or_else(|| {
        Error::message(format!(
            "unsupported target `{target_name}` (try `notwig-ext-cli targets`)"
        ))
    })?;
    let source_dir = resolve_source_dir(path)?;
    read_manifest(&source_dir)?;
    let package = package_metadata(&source_dir)?;
    ensure_target_installed(target.rust_triple, install_targets)?;
    build_package(&source_dir, &package, target)?;
    let artifact = artifact_path(&package, target);
    if !artifact.is_file() {
        return Err(Error::message(format!(
            "cargo did not produce the expected entrypoint: {}",
            artifact.display()
        )));
    }
    println!(
        "built {} entrypoint: {}",
        target.canonical,
        artifact.display()
    );
    Ok(artifact)
}

/// Packages whichever target binaries already exist in Cargo's target directory.
pub fn package_targets(
    target_names: &[String],
    path: Option<&Path>,
    out_dir: &Path,
    name: Option<&str>,
) -> Result<PathBuf> {
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
        return Ok(build_target::all().to_vec());
    }
    let mut targets = Vec::new();
    for raw in target_names {
        let target = build_target::resolve(raw).ok_or_else(|| {
            Error::message(format!(
                "unsupported target `{raw}` (try `notwig-ext-cli targets`)"
            ))
        })?;
        if !targets.contains(&target) {
            targets.push(target);
        }
    }
    Ok(targets)
}

fn read_manifest(source_dir: &Path) -> Result<ExtensionManifest> {
    let manifest_path = source_dir.join(MANIFEST_NAME);
    let text =
        fs::read_to_string(&manifest_path).map_err(|error| Error::io(&manifest_path, error))?;
    let manifest: ExtensionManifest = serde_json::from_str(&text)?;
    manifest
        .validate_source()
        .map_err(|error| Error::message(format!("{}: {error}", manifest_path.display())))?;
    Ok(manifest)
}

fn artifact_path(package: &PackageMetadata, target: BuildTarget) -> PathBuf {
    package
        .target_directory
        .join(target.rust_triple)
        .join("release")
        .join(target.artifact_name(&package.target_stem))
}

fn resolve_source_dir(explicit: Option<&Path>) -> Result<PathBuf> {
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

#[derive(Debug)]
struct PackageMetadata {
    target_stem: String,
    target_directory: PathBuf,
}

fn package_metadata(source_dir: &Path) -> Result<PackageMetadata> {
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

fn cargo_binary() -> std::ffi::OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
}

fn build_package(source_dir: &Path, package: &PackageMetadata, target: BuildTarget) -> Result<()> {
    let cargo = cargo_binary();
    let status = Command::new(cargo)
        .arg("build")
        .arg("--release")
        .arg("--manifest-path")
        .arg(source_dir.join("Cargo.toml"))
        .arg("--target")
        .arg(target.rust_triple)
        .status()
        .map_err(|error| Error::message(format!("run cargo build: {error}")))?;
    if !status.success() {
        return Err(Error::message(format!(
            "cargo build failed for {} ({}). Install the Rust target with `rustup target add {}` or build on that target platform.",
            target.canonical, package.target_stem, target.rust_triple
        )));
    }
    Ok(())
}

fn ensure_target_installed(triple: &str, install: bool) -> Result<()> {
    if !install {
        return Ok(());
    }
    let rustup = std::env::var_os("RUSTUP").unwrap_or_else(|| "rustup".into());
    let output = match Command::new(&rustup)
        .args(["target", "list", "--installed"])
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => return Ok(()),
    };
    let text = String::from_utf8_lossy(&output.stdout);
    if text.split_whitespace().any(|target| target == triple) {
        return Ok(());
    }
    eprintln!("installing missing Rust target {triple}");
    let status = Command::new(&rustup)
        .args(["target", "add", triple])
        .status()
        .map_err(|error| Error::message(format!("run rustup target add: {error}")))?;
    if !status.success() {
        return Err(Error::message(format!(
            "could not install Rust target `{triple}`"
        )));
    }
    Ok(())
}

fn sanitized_id(manifest: &ExtensionManifest) -> String {
    manifest
        .id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect()
}

fn default_zip_name(manifest: &ExtensionManifest, target: BuildTarget) -> String {
    format!("{}-{}.zip", sanitized_id(manifest), target.canonical)
}

fn ensure_zip_extension(file_name: String) -> Result<String> {
    let file_name = if Path::new(&file_name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
    {
        file_name
    } else {
        format!("{file_name}.zip")
    };
    if Path::new(&file_name).components().count() != 1 {
        return Err(Error::message("--name must be a file name, not a path"));
    }
    Ok(file_name)
}

#[derive(Debug, Clone)]
pub struct PackageEntry {
    pub target: BuildTarget,
    pub entrypoint: String,
    pub bytes: Vec<u8>,
}

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

fn safe_relative(value: &str) -> Result<String> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(Error::message(format!(
            "package path must be relative and stay inside the extension: {value}"
        )));
    }
    Ok(path
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/"))
}
