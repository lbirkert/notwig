use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use notwig_ext_api::manifest::{ExtensionEntryPoint, ExtensionInclude, ExtensionManifest};
use serde_json::Value;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::error::{Error, Result};
use crate::target::{self, BuildTarget};

const MANIFEST_NAME: &str = "extension.json";
const DEFAULT_ICON: &str = "icon.png";

pub fn command(args: &[String]) -> Result<()> {
    let mut target_arg = None;
    let mut path = None;
    let mut out_dir = PathBuf::from("dist");
    let mut name = None;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--path" | "--manifest-path" => {
                path = Some(PathBuf::from(take_value(args, &mut index, "--path")?));
            }
            "--out-dir" => {
                out_dir = PathBuf::from(take_value(args, &mut index, "--out-dir")?);
            }
            "--name" => {
                name = Some(take_value(args, &mut index, "--name")?);
            }
            "--target" => {
                target_arg = Some(take_value(args, &mut index, "--target")?);
            }
            flag if flag.starts_with('-') => {
                return Err(Error::message(format!("unknown option: {flag}")));
            }
            value => {
                if target_arg.replace(value.to_owned()).is_some() {
                    return Err(Error::message("build accepts exactly one target"));
                }
            }
        }
        index += 1;
    }

    let target_name =
        target_arg.ok_or_else(|| Error::message("build requires a target (try `targets`)"))?;
    let target = target::resolve(&target_name).ok_or_else(|| {
        Error::message(format!(
            "unsupported target `{target_name}` (try `notwig-ext-cli targets`)"
        ))
    })?;
    let source_dir = resolve_source_dir(path.as_deref())?;
    let manifest_path = source_dir.join(MANIFEST_NAME);
    let text =
        fs::read_to_string(&manifest_path).map_err(|error| Error::io(&manifest_path, error))?;
    let manifest: ExtensionManifest = serde_json::from_str(&text)?;
    manifest
        .validate_source()
        .map_err(|error| Error::message(format!("{}: {error}", manifest_path.display())))?;

    let package = package_metadata(&source_dir)?;
    build_package(&source_dir, &package, target)?;
    let artifact = package
        .target_directory
        .join(target.rust_triple)
        .join("release")
        .join(target.artifact_name(&package.target_stem));
    let artifact_bytes = fs::read(&artifact).map_err(|error| Error::io(&artifact, error))?;
    let entrypoint = target.entrypoint_name(&package.target_stem);

    let file_name = name.unwrap_or_else(|| default_zip_name(&manifest, target));
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
    if out_dir.as_os_str().is_empty() {
        return Err(Error::message("--out-dir must not be empty"));
    }
    fs::create_dir_all(&out_dir).map_err(|error| Error::io(&out_dir, error))?;
    let out_path = out_dir.join(file_name);
    package_extension(
        &manifest,
        &source_dir,
        target,
        &entrypoint,
        &artifact_bytes,
        &out_path,
    )?;
    println!("packaged {}", out_path.display());
    Ok(())
}

fn take_value(args: &[String], index: &mut usize, flag: &str) -> Result<String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| Error::message(format!("{flag} requires a value")))
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
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
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

fn build_package(source_dir: &Path, package: &PackageMetadata, target: BuildTarget) -> Result<()> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = Command::new(&cargo)
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
            "cargo build failed for {} ({})",
            target.canonical, package.target_stem
        )));
    }
    Ok(())
}

fn default_zip_name(manifest: &ExtensionManifest, target: BuildTarget) -> String {
    let id = manifest
        .id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    format!("{id}-{}.zip", target.canonical)
}

pub fn package_extension(
    manifest: &ExtensionManifest,
    source_dir: &Path,
    target: BuildTarget,
    entrypoint: &str,
    artifact: &[u8],
    out_path: &Path,
) -> Result<()> {
    let includes = manifest.includes.clone();
    let mut manifest = manifest.clone();
    manifest.includes.clear();
    manifest.entrypoints = vec![ExtensionEntryPoint {
        entrypoint: entrypoint.to_owned(),
        target: target.canonical.to_owned(),
    }];

    let mut entries = BTreeMap::new();
    entries.insert(entrypoint.to_owned(), artifact.to_vec());

    let default_icon = source_dir.join(DEFAULT_ICON);
    if default_icon.is_file() {
        let bytes = fs::read(&default_icon).map_err(|error| Error::io(&default_icon, error))?;
        entries.insert(DEFAULT_ICON.to_owned(), bytes);
    }
    if manifest.image.is_none() && entries.contains_key(DEFAULT_ICON) {
        manifest.image = Some(DEFAULT_ICON.to_owned());
    }
    if let Some(image) = &manifest.image {
        let path = safe_relative(image)?;
        let source = source_dir.join(&path);
        let bytes = fs::read(&source).map_err(|error| Error::io(&source, error))?;
        entries.insert(path, bytes);
    }

    for include in includes {
        if !include_applies(&include, target) {
            continue;
        }
        let path = safe_relative(&include.path)?;
        if path == MANIFEST_NAME || path == entrypoint {
            return Err(Error::message(format!(
                "include path conflicts with generated package file: {path}"
            )));
        }
        let source = source_dir.join(&path);
        let bytes = fs::read(&source).map_err(|error| Error::io(&source, error))?;
        match entries.get(&path) {
            Some(existing) if existing == &bytes => {}
            Some(_) => {
                return Err(Error::message(format!(
                    "include path conflicts with another package file: {path}"
                )));
            }
            None => {
                entries.insert(path, bytes);
            }
        }
    }

    let mut json = serde_json::to_vec_pretty(&manifest)?;
    json.push(b'\n');
    entries.insert(MANIFEST_NAME.to_owned(), json);

    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent).map_err(|error| Error::io(parent, error))?;
    }
    let file = File::create(out_path).map_err(|error| Error::io(out_path, error))?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);
    for (name, bytes) in entries {
        writer.start_file(name, options)?;
        writer
            .write_all(&bytes)
            .map_err(|error| Error::io(out_path, error))?;
    }
    writer.finish()?;
    Ok(())
}

fn include_applies(include: &ExtensionInclude, target: BuildTarget) -> bool {
    include
        .target
        .as_ref()
        .is_none_or(|wanted| notwig_ext_api::target::canonical(wanted) == Some(target.canonical))
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
