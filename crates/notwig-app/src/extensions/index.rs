use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

use notwig_ext_api::{ExtensionIndexEntry, ExtensionIndexUrl};
use sha2::{Digest, Sha256};

pub(crate) const MAX_INDEX_BYTES: usize = 4 * 1024 * 1024;

pub(crate) struct IndexFetchResult {
    pub entries: Vec<ExtensionIndexEntry>,
    pub errors: Vec<String>,
    pub images: BTreeMap<String, Vec<u8>>,
}

pub(crate) fn merge_indexes(documents: Vec<(String, Vec<u8>)>) -> IndexFetchResult {
    let mut entries: BTreeMap<String, ExtensionIndexEntry> = BTreeMap::new();
    let mut errors = Vec::new();
    for (source, bytes) in documents {
        let list: Vec<ExtensionIndexEntry> = match serde_json::from_slice(&bytes) {
            Ok(list) => list,
            Err(error) => {
                errors.push(format!("{source}: {error}"));
                continue;
            }
        };
        for mut entry in list {
            if let Err(error) = resolve_entry_urls(&mut entry, &source) {
                errors.push(format!("{source}: {}: {error}", entry.id));
                continue;
            }
            if let Err(error) = resolve_entry_image(&mut entry, &source) {
                errors.push(format!("{source}: {}: {error}", entry.id));
                entry.image = None;
            }
            if let Err(error) = entry.validate() {
                errors.push(format!("{source}: {}: {error}", entry.id));
                continue;
            }
            match entries.get_mut(&entry.id) {
                Some(existing) => merge_entry(existing, entry),
                None => {
                    entries.insert(entry.id.clone(), entry);
                }
            }
        }
    }
    IndexFetchResult {
        entries: entries.into_values().collect(),
        errors,
        images: BTreeMap::new(),
    }
}

pub(crate) fn select_download(entry: &ExtensionIndexEntry) -> Option<&ExtensionIndexUrl> {
    entry.download_for_host()
}

pub(crate) fn verify_sha256(bytes: &[u8], expected: &str) -> Result<(), String> {
    let actual = sha256_hex(bytes);
    if actual.eq_ignore_ascii_case(expected.trim()) {
        Ok(())
    } else {
        Err(format!(
            "downloaded extension hash mismatch (expected {}, got {actual})",
            expected.trim()
        ))
    }
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

pub(crate) fn normalize_indexes(indexes: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    indexes
        .into_iter()
        .map(|index| index.trim().to_owned())
        .filter(|index| !index.is_empty())
        .filter(|index| seen.insert(index.clone()))
        .collect()
}

fn merge_entry(existing: &mut ExtensionIndexEntry, candidate: ExtensionIndexEntry) {
    use std::cmp::Ordering;

    match version_cmp(&candidate.version, &existing.version) {
        Ordering::Greater => *existing = candidate,
        Ordering::Equal => {
            for download in candidate.urls {
                if let Some(current) = existing
                    .urls
                    .iter_mut()
                    .find(|current| current.url == download.url)
                {
                    *current = download;
                } else {
                    existing.urls.push(download);
                }
            }
        }
        Ordering::Less => {}
    }
}

fn version_cmp(candidate: &str, existing: &str) -> std::cmp::Ordering {
    match (
        semver::Version::parse(candidate),
        semver::Version::parse(existing),
    ) {
        (Ok(candidate), Ok(existing)) => candidate.cmp(&existing),
        _ => std::cmp::Ordering::Greater,
    }
}

fn resolve_entry_urls(entry: &mut ExtensionIndexEntry, index_source: &str) -> Result<(), String> {
    for download in &mut entry.urls {
        resolve_url(&mut download.url, index_source)?;
    }
    Ok(())
}

fn resolve_entry_image(entry: &mut ExtensionIndexEntry, index_source: &str) -> Result<(), String> {
    if let Some(image) = &mut entry.image {
        resolve_url(image, index_source)?;
    }
    Ok(())
}

fn resolve_url(value: &mut String, index_source: &str) -> Result<(), String> {
    if let Ok(url) = url::Url::parse(value) {
        if url.has_host() || url.scheme() == "file" {
            return Ok(());
        }
    }
    if let Ok(base) = url::Url::parse(index_source) {
        *value = base
            .join(value)
            .map_err(|error| format!("resolve url: {error}"))?
            .to_string();
        return Ok(());
    }
    let base = Path::new(index_source).parent().unwrap_or(Path::new("."));
    *value = base.join(&value).to_string_lossy().into_owned();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn entry(url: &str, target: &str) -> String {
        format!(
            r#"[{{
                "id": "com.test.gain",
                "name": "Test Gain",
                "vendor": "Test",
                "version": "0.1.0",
                "api_version": 8,
                "urls": [{{ "url": "{url}", "hash": "{HASH}", "targets": ["{target}"] }}],
                "devices": [{{ "name": "Gain", "type_id": "com.test.gain.fx" }}]
            }}]"#
        )
    }

    #[test]
    fn merges_target_entries_with_the_same_id() {
        let result = merge_indexes(vec![
            (
                "https://example.com/extensions.json".into(),
                entry("macos.zip", "macos-aarch64").into_bytes(),
            ),
            (
                "https://example.com/extensions.json".into(),
                entry("wasm.zip", "wasm").into_bytes(),
            ),
        ]);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].urls.len(), 2);
        assert_eq!(
            result.entries[0]
                .download_for(|target| target == "wasm")
                .map(|download| download.url.as_str()),
            Some("https://example.com/wasm.zip")
        );
    }

    #[test]
    fn resolves_index_images_relative_to_the_index() {
        let document = format!(
            r#"[{{
                "id": "com.test.gain",
                "name": "Test Gain",
                "vendor": "Test",
                "version": "0.1.0",
                "api_version": 8,
                "urls": [{{ "url": "gain.zip", "hash": "{HASH}", "targets": ["wasm"] }}],
                "devices": [],
                "image": "extensions/com.test.gain.png"
            }}]"#
        );
        let result = merge_indexes(vec![(
            "https://example.com/extensions.json".into(),
            document.into_bytes(),
        )]);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert_eq!(
            result.entries[0].image.as_deref(),
            Some("https://example.com/extensions/com.test.gain.png")
        );
    }

    #[test]
    fn same_version_url_hash_updates_replace_the_previous_hash() {
        let newer = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        let older = format!(
            r#"[{{
                "id": "com.test.gain",
                "name": "Test Gain",
                "vendor": "Test",
                "version": "0.1.0",
                "api_version": 8,
                "urls": [{{ "url": "gain.zip", "hash": "{HASH}", "targets": ["wasm"] }}],
                "devices": []
            }}]"#
        );
        let updated = older.replace(HASH, newer);
        let result = merge_indexes(vec![
            (
                "https://example.com/extensions.json".into(),
                older.into_bytes(),
            ),
            (
                "https://example.com/extensions.json".into(),
                updated.into_bytes(),
            ),
        ]);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert_eq!(result.entries[0].urls.len(), 1);
        assert_eq!(result.entries[0].urls[0].hash, newer);
    }

    #[test]
    fn verifies_sha256_case_insensitively() {
        let hash = sha256_hex(b"notwig");
        verify_sha256(b"notwig", &hash).unwrap();
        verify_sha256(b"notwig", &hash.to_ascii_uppercase()).unwrap();
        assert!(verify_sha256(b"other", &hash).is_err());
    }
}
