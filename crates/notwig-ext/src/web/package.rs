use std::io::{Cursor, Read};

use notwig_ext_api::ExtensionManifest;
use zip::ZipArchive;

use crate::error::ExtensionError;

const MAX_MODULE_BYTES: usize = 8 * 1024 * 1024;
const MAX_MANIFEST_BYTES: usize = 1024 * 1024;

pub struct WebExtensionPackage {
    pub(crate) manifest: ExtensionManifest,
    pub(crate) wasm: Vec<u8>,
}

impl WebExtensionPackage {
    pub fn from_zip(bytes: &[u8]) -> Result<Self, ExtensionError> {
        let mut archive = ZipArchive::new(Cursor::new(bytes))
            .map_err(|error| ExtensionError::Archive(error.to_string()))?;
        let mut manifest_name = None;
        let mut entries = Vec::new();
        for index in 0..archive.len() {
            let file = archive
                .by_index(index)
                .map_err(|error| ExtensionError::Archive(error.to_string()))?;
            let name = file.name().replace('\\', "/");
            if name == "extension.json"
                || (manifest_name.is_none()
                    && name.ends_with("/extension.json")
                    && name.split('/').count() == 2)
            {
                manifest_name = Some(name.clone());
            }
            entries.push(name);
        }
        let manifest_name = manifest_name.ok_or(ExtensionError::MissingManifest)?;
        let base = manifest_name
            .strip_suffix("extension.json")
            .unwrap_or_default()
            .to_owned();
        let manifest_bytes = read_entry(&mut archive, &manifest_name, MAX_MANIFEST_BYTES)?;
        let manifest: ExtensionManifest = serde_json::from_slice(&manifest_bytes)
            .map_err(|error| ExtensionError::Manifest(error.to_string()))?;
        manifest
            .validate()
            .map_err(|error| ExtensionError::Manifest(error.to_string()))?;
        if manifest.entry.starts_with('/')
            || manifest.entry.contains('\\')
            || manifest
                .entry
                .split('/')
                .any(|part| part == ".." || part == ".")
        {
            return Err(ExtensionError::Manifest(
                "extension entry must stay inside the package".into(),
            ));
        }
        let entry = format!("{base}{}", manifest.entry);
        if !entries.iter().any(|name| name == &entry) {
            return Err(ExtensionError::MissingExport(manifest.entry));
        }
        let wasm = read_entry(&mut archive, &entry, MAX_MODULE_BYTES)?;
        if !wasm.starts_with(b"\0asm") {
            return Err(ExtensionError::Manifest(
                "entry is not a wasm module".into(),
            ));
        }
        Ok(Self { manifest, wasm })
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

fn read_entry(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    name: &str,
    limit: usize,
) -> Result<Vec<u8>, ExtensionError> {
    let mut file = archive
        .by_name(name)
        .map_err(|error| ExtensionError::Archive(error.to_string()))?;
    if file.size() > limit as u64 {
        return Err(ExtensionError::ArchiveTooLarge);
    }
    let mut bytes = Vec::with_capacity(file.size() as usize);
    file.read_to_end(&mut bytes)
        .map_err(|error| ExtensionError::Archive(error.to_string()))?;
    Ok(bytes)
}
