use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use notwig_ext_api::{ExtensionManifest, target};

use crate::archive::{ExtensionArchive, read_archive, read_extension_files};
use crate::error::ExtensionError;

/// A browser-ready extension package containing its WASM entrypoint.
pub struct WebExtensionPackage {
    pub(crate) manifest: ExtensionManifest,
    pub(crate) wasm: Vec<u8>,
}

impl WebExtensionPackage {
    /// Builds a package from ZIP bytes, selecting the WASM entrypoint.
    pub fn from_zip(bytes: &[u8]) -> Result<Self, ExtensionError> {
        Self::from_archive(read_archive(bytes, None)?)
    }

    /// Builds a package from an already extracted file tree.
    pub fn from_files(files: BTreeMap<PathBuf, Vec<u8>>) -> Result<Self, ExtensionError> {
        Self::from_archive(read_extension_files(files, None)?)
    }

    /// Builds a package from a validated archive.
    ///
    /// Requires a `wasm` entrypoint that stays inside the package and starts
    /// with the WebAssembly magic bytes.
    pub fn from_archive(archive: ExtensionArchive) -> Result<Self, ExtensionError> {
        let entry = archive.manifest.entry_for(target::is_wasm).ok_or_else(|| {
            ExtensionError::Manifest(
                "extension has no wasm entrypoint for the browser build".into(),
            )
        })?;
        let entrypoint = entry.entrypoint.as_str();
        if entrypoint.starts_with('/')
            || entrypoint.contains('\\')
            || entrypoint
                .split('/')
                .any(|part| part == ".." || part == ".")
        {
            return Err(ExtensionError::Manifest(
                "extension entrypoint must stay inside the package".into(),
            ));
        }
        let wasm = archive
            .files
            .get(Path::new(entrypoint))
            .cloned()
            .ok_or_else(|| ExtensionError::MissingExport(entrypoint.to_owned()))?;
        if !wasm.starts_with(b"\0asm") {
            return Err(ExtensionError::Manifest(
                "entrypoint is not a wasm module".into(),
            ));
        }
        Ok(Self {
            manifest: archive.manifest,
            wasm,
        })
    }

    /// Returns the extension id.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.manifest.id
    }

    /// Returns the extension display name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.manifest.name
    }

    /// Returns the extension version.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.manifest.version
    }
}
