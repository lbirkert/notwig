use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use notwig_ext_api::ExtensionManifest;

use crate::archive::{ExtensionArchive, read_archive, read_extension_files};
use crate::error::ExtensionError;

pub struct WebExtensionPackage {
    pub(crate) manifest: ExtensionManifest,
    pub(crate) wasm: Vec<u8>,
}

impl WebExtensionPackage {
    pub fn from_zip(bytes: &[u8]) -> Result<Self, ExtensionError> {
        Self::from_archive(read_archive(bytes, None)?)
    }

    pub fn from_files(files: BTreeMap<PathBuf, Vec<u8>>) -> Result<Self, ExtensionError> {
        Self::from_archive(read_extension_files(files, None)?)
    }

    pub fn from_archive(archive: ExtensionArchive) -> Result<Self, ExtensionError> {
        let entry = &archive.manifest.entry;
        if entry.starts_with('/')
            || entry.contains('\\')
            || entry.split('/').any(|part| part == ".." || part == ".")
        {
            return Err(ExtensionError::Manifest(
                "extension entry must stay inside the package".into(),
            ));
        }
        let wasm = archive
            .files
            .get(Path::new(entry))
            .cloned()
            .ok_or_else(|| ExtensionError::MissingExport(entry.clone()))?;
        if !wasm.starts_with(b"\0asm") {
            return Err(ExtensionError::Manifest(
                "entry is not a wasm module".into(),
            ));
        }
        Ok(Self {
            manifest: archive.manifest,
            wasm,
        })
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.manifest.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.manifest.name
    }

    #[must_use]
    pub fn version(&self) -> &str {
        &self.manifest.version
    }
}
