use crate::CoreError;
use crate::domain::{AssetEntry, ProjectDoc};
use crate::ids::AssetId;
use crate::protocol::{DocChange, EngineMsg};

use super::dispatch::{Changes, not_found};

pub(super) fn register_asset(
    doc: &mut ProjectDoc,
    asset_id: AssetId,
    entry: &AssetEntry,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    if entry.id != asset_id {
        return Err(CoreError::ValidationFailed {
            reason: "asset id does not match entry id".into(),
        });
    }
    if doc.assets.assets.contains_key(&asset_id) {
        return Err(CoreError::DuplicateId {
            id: asset_id.to_string(),
        });
    }
    doc.assets.assets.insert(asset_id, entry.clone());
    changes.affected.assets.insert(asset_id);
    changes.push(DocChange::AssetUpserted(asset_id));
    Ok(Vec::new())
}

pub(super) fn replace_asset(
    doc: &mut ProjectDoc,
    asset_id: AssetId,
    entry: &AssetEntry,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    if entry.id != asset_id {
        return Err(CoreError::ValidationFailed {
            reason: "asset id does not match entry id".into(),
        });
    }
    if !doc.assets.assets.contains_key(&asset_id) {
        return Err(not_found(&asset_id));
    }
    doc.assets.assets.insert(asset_id, entry.clone());
    changes.affected.assets.insert(asset_id);
    changes.push(DocChange::AssetUpserted(asset_id));
    Ok(Vec::new())
}

pub(super) fn relink_asset(
    doc: &mut ProjectDoc,
    asset_id: AssetId,
    uri: &str,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    let entry = doc
        .assets
        .assets
        .get_mut(&asset_id)
        .ok_or_else(|| not_found(&asset_id))?;
    uri.clone_into(&mut entry.uri);
    changes.affected.assets.insert(asset_id);
    changes.push(DocChange::AssetUpserted(asset_id));
    Ok(Vec::new())
}
