use std::collections::BTreeMap;
use std::path::PathBuf;

use notwig_ui::Shortcuts;

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

fn config_path() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    #[cfg(target_os = "macos")]
    let path = home.join("Library/Application Support/notwig/shortcuts.json");
    #[cfg(not(target_os = "macos"))]
    let path = home.join(".config/notwig/shortcuts.json");
    Some(path)
}
