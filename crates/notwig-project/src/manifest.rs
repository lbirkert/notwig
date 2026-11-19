use notwig_core::{AssetEntry, ProjectDoc, SCHEMA_VERSION, SchemaVersion};
use serde::{Deserialize, Serialize};

/// On-disk wrapper around a [`ProjectDoc`].
///
/// `doc` is authoritative. The top-level `schema`, `assets`, `created_utc`, and
/// `modified_utc` fields are advisory copies kept for external inspection; load
/// validates `schema` against `doc.schema` and reads project state only from
/// `doc`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: SchemaVersion,
    pub app_version: Option<String>,
    pub doc_path: String,
    pub assets: Vec<AssetEntry>,
    pub created_utc: i64,
    pub modified_utc: i64,
    pub doc: ProjectDoc,
}

impl Manifest {
    #[must_use]
    pub fn new(doc: &ProjectDoc) -> Self {
        Self {
            schema: SCHEMA_VERSION,
            app_version: Some(env!("CARGO_PKG_VERSION").into()),
            doc_path: "project.json".into(),
            assets: doc.assets.assets.values().cloned().collect(),
            created_utc: doc.meta.created_utc,
            modified_utc: doc.meta.modified_utc,
            doc: doc.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestIssue {
    pub entity: String,
    pub message: String,
}
