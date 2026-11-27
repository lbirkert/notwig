//! VST3 bundle discovery and metadata scanning.

use notwig_rack_core::error::Result;
use notwig_rack_core::info::{PluginCategory, PluginInfo};
use notwig_rack_core::scanner::PluginScanner;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use vst3::Steinberg::{IPluginFactoryTrait, PClassInfo, PClassInfo_, TUID, kResultOk};

use crate::module::LoadedModule;
use crate::plugin::Vst3Plugin;
use crate::{FORMAT, VST3_EXTENSION};

/// VST3 scanner.
#[derive(Debug, Default)]
pub struct Vst3Scanner;

impl Vst3Scanner {
    /// Construct a default scanner.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl PluginScanner for Vst3Scanner {
    type Plugin = Vst3Plugin;

    fn scan(&self) -> Result<Vec<PluginInfo>> {
        let mut out = Vec::new();
        for dir in default_vst3_paths() {
            if dir.exists() {
                scan_dir_into(&dir, &mut out);
            }
        }
        Ok(out)
    }

    fn scan_path(&self, path: &Path) -> Result<Vec<PluginInfo>> {
        let mut out = Vec::new();
        if path.exists() {
            scan_dir_into(path, &mut out);
        }
        Ok(out)
    }

    fn load(&self, info: &PluginInfo) -> Result<Self::Plugin> {
        Vst3Plugin::load_from(info)
    }
}

/// Standard VST3 install locations for the current OS.
#[must_use]
pub fn default_vst3_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        let mut user = PathBuf::from(home);
        #[cfg(target_os = "macos")]
        user.push("Library/Audio/Plug-Ins/VST3");
        #[cfg(target_os = "linux")]
        user.push(".vst3");
        #[cfg(target_os = "windows")]
        user.push("AppData/Local/Programs/Common/VST3");
        out.push(user);
    }
    #[cfg(target_os = "macos")]
    out.push(PathBuf::from("/Library/Audio/Plug-Ins/VST3"));
    #[cfg(target_os = "linux")]
    out.push(PathBuf::from("/usr/lib/vst3"));
    #[cfg(target_os = "windows")]
    {
        if let Some(pf) = std::env::var_os("CommonProgramFiles") {
            let mut p = PathBuf::from(pf);
            p.push("VST3");
            out.push(p);
        }
    }
    out
}

fn scan_dir_into(dir: &Path, out: &mut Vec<PluginInfo>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if !name.ends_with(VST3_EXTENSION) {
            continue;
        }
        if let Err(err) = scan_bundle_into(&path, out) {
            tracing::debug!(path = %path.display(), error = %err, "skipping VST3 plugin");
        }
    }
}

fn scan_bundle_into(bundle: &Path, out: &mut Vec<PluginInfo>) -> Result<()> {
    let module = unsafe { LoadedModule::open(bundle) }?;
    let factory = module.factory()?;
    let count = unsafe { factory.countClasses() };
    let mut info = empty_pclass_info();
    let mut audio_classes = Vec::new();
    let mut ara_classes = BTreeSet::new();
    for idx in 0..count {
        if unsafe { factory.getClassInfo(idx, &raw mut info) } != kResultOk {
            continue;
        }
        let category = char8_array_to_string(&info.category);
        let name = char8_array_to_string(&info.name);
        match category.as_str() {
            "Audio Module Class" => audio_classes.push((name, tuid_to_hex(&info.cid))),
            "ARA Main Factory Class" => {
                ara_classes.insert(name);
            }
            _ => {}
        }
    }
    for (name, unique_id) in audio_classes {
        let supports_ara = ara_classes.contains(&name);
        out.push(PluginInfo {
            name,
            vendor: String::new(),
            version: 0,
            category: PluginCategory::Effect,
            path: bundle.to_path_buf(),
            unique_id,
            format: FORMAT,
            has_editor: false,
            accepts_midi: false,
            supports_ara,
        });
    }
    Ok(())
}

fn empty_pclass_info() -> PClassInfo {
    PClassInfo {
        cid: [0; 16],
        cardinality: 0,
        category: [0; PClassInfo_::kCategorySize as usize],
        name: [0; PClassInfo_::kNameSize as usize],
    }
}

#[allow(clippy::cast_sign_loss)]
pub(crate) fn char8_array_to_string(array: &[i8]) -> String {
    // VST3 char8 is signed on Apple, unsigned elsewhere; the cast
    // preserves bit pattern.
    let bytes: Vec<u8> = array
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| b as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[allow(clippy::cast_sign_loss)]
pub(crate) fn tuid_to_hex(cid: &[i8; 16]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(32);
    for &b in cid {
        let _ = write!(s, "{:02x}", b as u8);
    }
    s
}

#[allow(clippy::cast_possible_wrap)]
pub(crate) fn hex_to_tuid(hex: &str) -> Option<TUID> {
    if hex.len() != 32 {
        return None;
    }
    let mut out: TUID = [0; 16];
    for i in 0..16 {
        out[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()? as i8;
    }
    Some(out)
}
