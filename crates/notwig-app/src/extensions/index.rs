use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use notwig_ext_api::ExtensionIndexEntry;

pub(crate) const MAX_INDEX_BYTES: usize = 4 * 1024 * 1024;

pub(crate) struct IndexFetchResult {
    pub entries: Vec<ExtensionIndexEntry>,
    pub errors: Vec<String>,
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
            if let Err(error) = resolve_entry_url(&mut entry, &source) {
                errors.push(format!("{source}: {}: {error}", entry.id));
                continue;
            }
            if let Err(error) = entry.validate() {
                errors.push(format!("{source}: {}: {error}", entry.id));
                continue;
            }
            match entries.get(&entry.id) {
                Some(existing) if !version_is_newer(&entry.version, &existing.version) => {}
                _ => {
                    entries.insert(entry.id.clone(), entry);
                }
            }
        }
    }
    IndexFetchResult {
        entries: entries.into_values().collect(),
        errors,
    }
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

fn resolve_entry_url(entry: &mut ExtensionIndexEntry, index_source: &str) -> Result<(), String> {
    if let Ok(url) = url::Url::parse(&entry.url) {
        if url.has_host() || url.scheme() == "file" {
            return Ok(());
        }
    }
    if let Ok(base) = url::Url::parse(index_source) {
        entry.url = base
            .join(&entry.url)
            .map_err(|error| format!("resolve url: {error}"))?
            .to_string();
        return Ok(());
    }
    let base = Path::new(index_source).parent().unwrap_or(Path::new("."));
    entry.url = base.join(&entry.url).to_string_lossy().into_owned();
    Ok(())
}

fn version_is_newer(candidate: &str, existing: &str) -> bool {
    match (
        semver::Version::parse(candidate),
        semver::Version::parse(existing),
    ) {
        (Ok(candidate), Ok(existing)) => candidate > existing,
        _ => true,
    }
}
