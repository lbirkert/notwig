//! Per-target Cargo compilation and artifact lookup.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{Error, Result};
use crate::target::{self as build_target, BuildTarget};

use super::source::{
    PackageMetadata, cargo_binary, package_metadata, read_manifest, resolve_source_dir,
};

/// Builds one target binary and leaves it in Cargo's target directory.
pub fn build_entrypoint(
    target_name: &str,
    path: Option<&Path>,
    install_targets: bool,
) -> Result<PathBuf> {
    let target = resolve_target(target_name)?;
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

pub(super) fn resolve_target(raw: &str) -> Result<BuildTarget> {
    build_target::resolve(raw).ok_or_else(|| {
        Error::message(format!(
            "unsupported target `{raw}` (try `notwig-ext-cli targets`)"
        ))
    })
}

pub(super) fn artifact_path(package: &PackageMetadata, target: BuildTarget) -> PathBuf {
    package
        .target_directory
        .join(target.rust_triple)
        .join("release")
        .join(target.artifact_name(&package.target_stem))
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
