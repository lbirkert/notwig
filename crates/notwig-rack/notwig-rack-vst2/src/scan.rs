//! VST 2 plugin discovery, path resolution, and shell enumeration support.

use crate::plugin::Vst2Plugin;
use notwig_rack_core::error::Result;
use notwig_rack_core::info::PluginInfo;
use notwig_rack_core::scanner::PluginScanner;
use std::ffi::{CString, OsStr};
use std::path::{Path, PathBuf};

/// Format identifier used on returned [`PluginInfo`] entries.
pub const FORMAT: &str = "vst2";

/// Filename extension used by VST 2 plugins on every platform.
pub const VST2_EXTENSION: &str = ".vst";

/// VST 2 scanner.
#[derive(Debug, Default)]
pub struct Vst2Scanner;

impl Vst2Scanner {
    /// Construct a default scanner.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl PluginScanner for Vst2Scanner {
    type Plugin = Vst2Plugin;

    fn scan(&self) -> Result<Vec<PluginInfo>> {
        let mut out = Vec::new();
        for dir in default_vst2_paths() {
            if dir.exists() {
                scan_path_into(&dir, &mut out, 0);
            }
        }
        dedupe(&mut out);
        Ok(out)
    }

    fn scan_path(&self, path: &Path) -> Result<Vec<PluginInfo>> {
        let mut out = Vec::new();
        scan_path_into(path, &mut out, 0);
        dedupe(&mut out);
        Ok(out)
    }

    fn load(&self, info: &PluginInfo) -> Result<Self::Plugin> {
        Vst2Plugin::load_from(info)
    }
}

/// Standard VST 2 install locations for the current OS.
#[must_use]
pub fn default_vst2_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();

    if let Some(paths) = std::env::var_os("VST_PATH") {
        out.extend(std::env::split_paths(&paths));
    }

    if let Some(home) = std::env::var_os("HOME") {
        let mut user = PathBuf::from(home);
        #[cfg(target_os = "macos")]
        user.push("Library/Audio/Plug-Ins/VST");
        #[cfg(target_os = "linux")]
        user.push(".vst");
        #[cfg(target_os = "windows")]
        user.push("VSTPlugins");
        out.push(user);
    }

    #[cfg(target_os = "macos")]
    out.push(PathBuf::from("/Library/Audio/Plug-Ins/VST"));

    #[cfg(target_os = "linux")]
    {
        out.push(PathBuf::from("/usr/lib/vst"));
        out.push(PathBuf::from("/usr/local/lib/vst"));
        if let Some(home) = std::env::var_os("HOME") {
            out.push(PathBuf::from(home).join(".lxvst"));
        }
        out.push(PathBuf::from("/usr/lib/lxvst"));
        out.push(PathBuf::from("/usr/local/lib/lxvst"));
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(program_files) = std::env::var_os("ProgramFiles") {
            out.push(PathBuf::from(program_files).join("VstPlugins"));
            out.push(
                PathBuf::from(program_files)
                    .join("Steinberg")
                    .join("VstPlugins"),
            );
        }
        if let Some(common) = std::env::var_os("CommonProgramFiles") {
            out.push(PathBuf::from(common).join("VST2"));
        }
    }

    out
}

fn dedupe(entries: &mut Vec<PluginInfo>) {
    entries.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then_with(|| a.unique_id.cmp(&b.unique_id))
    });
    entries.dedup_by(|a, b| a.path == b.path && a.unique_id == b.unique_id);
}

fn scan_path_into(path: &Path, out: &mut Vec<PluginInfo>, depth: usize) {
    if depth > 8 {
        return;
    }
    if path.is_dir() {
        if is_plugin_bundle(path) {
            scan_bundle_into(path, out);
            return;
        }
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            scan_path_into(&entry.path(), out, depth + 1);
        }
    } else if is_plugin_file(path) {
        scan_bundle_into(path, out);
    }
}

fn scan_bundle_into(path: &Path, out: &mut Vec<PluginInfo>) {
    match Vst2Plugin::load_path(path, 0) {
        Ok(plugin) => out.extend(plugin.class_infos()),
        Err(err) => {
            tracing::debug!(path = %path.display(), error = %err, "skipping VST2 plugin");
        }
    }
}

fn is_plugin_bundle(path: &Path) -> bool {
    has_extension(path, VST2_EXTENSION)
}

fn is_plugin_file(path: &Path) -> bool {
    if has_extension(path, VST2_EXTENSION) {
        return true;
    }
    #[cfg(target_os = "windows")]
    if has_extension(path, ".dll") {
        return true;
    }
    #[cfg(target_os = "linux")]
    if has_extension(path, ".so") {
        return true;
    }
    false
}

fn has_extension(path: &Path, extension: &str) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|ext| ext.eq_ignore_ascii_case(extension.trim_start_matches('.')))
}

pub(crate) fn resolve_binary(path: &Path) -> PathBuf {
    if !path.is_dir() {
        return path.to_path_buf();
    }

    #[cfg(target_os = "macos")]
    {
        let stem = path.file_stem().unwrap_or_default();
        let candidate = path.join("Contents").join("MacOS").join(stem);
        if candidate.exists() {
            return candidate;
        }
        let candidate = path.join(stem);
        if candidate.exists() {
            return candidate;
        }
    }

    path.to_path_buf()
}

pub(crate) fn plugin_directory(path: &Path) -> Option<CString> {
    let parent = path.parent()?;
    #[cfg(target_os = "macos")]
    {
        // macOS hosts traditionally answer with a legacy FSRef; return none.
        let _ = parent;
        None
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use std::os::unix::ffi::OsStrExt;
        CString::new(parent.as_os_str().as_bytes()).ok()
    }
    #[cfg(target_os = "windows")]
    {
        CString::new(parent.to_string_lossy().as_bytes()).ok()
    }
}
