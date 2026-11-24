use std::collections::BTreeMap;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;

use notwig_ui::Shortcuts;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn load() -> Shortcuts {
    let Some(path) = config_path() else {
        return Shortcuts::default();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Shortcuts::default();
    };
    let Ok(config) = serde_json::from_str::<BTreeMap<String, Vec<String>>>(&text) else {
        tracing::warn!(path = %path.display(), "ignoring invalid shortcut config");
        return Shortcuts::default();
    };
    Shortcuts::from_config(&config)
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn save(shortcuts: &Shortcuts) -> Result<(), String> {
    let Some(path) = config_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let text = serde_json::to_string_pretty(&shortcuts.to_config())
        .map_err(|err| format!("encode shortcuts: {err}"))?;
    std::fs::write(&path, text).map_err(|err| format!("save shortcuts: {err}"))
}

#[cfg(not(target_arch = "wasm32"))]
fn config_path() -> Option<PathBuf> {
    crate::app_paths::data_dir().map(|dir| dir.join("shortcuts.json"))
}

#[cfg(target_arch = "wasm32")]
const WEB_KEY: &str = "notwig.shortcuts";

#[cfg(target_arch = "wasm32")]
pub(crate) fn load() -> Shortcuts {
    let Some(storage) = web_storage() else {
        return Shortcuts::default();
    };
    let Ok(Some(text)) = storage.get_item(WEB_KEY) else {
        return Shortcuts::default();
    };
    let Ok(config) = serde_json::from_str::<BTreeMap<String, Vec<String>>>(&text) else {
        return Shortcuts::default();
    };
    Shortcuts::from_config(&config)
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn save(shortcuts: &Shortcuts) -> Result<(), String> {
    let Some(storage) = web_storage() else {
        return Ok(());
    };
    let text = serde_json::to_string(&shortcuts.to_config())
        .map_err(|err| format!("encode shortcuts: {err}"))?;
    storage
        .set_item(WEB_KEY, &text)
        .map_err(|_| "could not save shortcuts".to_string())
}

#[cfg(target_arch = "wasm32")]
fn web_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}
