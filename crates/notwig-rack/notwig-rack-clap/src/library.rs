//! Plugin discovery and dynamic-library lifetime management.
//!
//! Owns the scanner, the per-platform search paths, bundle inspection, and the
//! RAII wrapper around the loaded `.clap` dylib and its `clap_entry` symbol.

use crate::plugin::ClapPlugin;
use crate::{CLAP_EXTENSION, FORMAT};

use notwig_rack_core::error::{Error, Result};
use notwig_rack_core::info::{PluginCategory, PluginInfo};
use notwig_rack_core::scanner::PluginScanner;

use clap_sys::entry::clap_plugin_entry;
use clap_sys::factory::plugin_factory::{CLAP_PLUGIN_FACTORY_ID, clap_plugin_factory};
use clap_sys::plugin::clap_plugin_descriptor;

use std::ffi::{CStr, CString, c_char};
use std::path::{Path, PathBuf};
use std::ptr;

/// Symbol name `clap_entry` plugins must export.
const ENTRY_SYMBOL: &[u8] = b"clap_entry\0";

/// CLAP scanner.
///
/// Walks the standard CLAP plugin directories for the current OS
/// and returns one entry per discovered CLAP plugin. Calling
/// [`PluginScanner::scan_path`] lets a host scan a custom
/// directory (useful for sandboxed test fixtures or
/// per-application bundled plugins).
#[derive(Debug, Default)]
pub struct ClapScanner;

impl ClapScanner {
    /// Construct a default scanner. There's no config state; the
    /// type exists so consumers have a stable handle to scan
    /// from.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl PluginScanner for ClapScanner {
    type Plugin = ClapPlugin;

    fn scan(&self) -> Result<Vec<PluginInfo>> {
        let mut out = Vec::new();
        for dir in default_clap_paths() {
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
        ClapPlugin::load_from(info)
    }
}

/// Standard locations the CLAP spec lists for each OS. Mirrors
/// what the CLAP example host walks.
#[must_use]
pub fn default_clap_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        let mut user = PathBuf::from(home);
        #[cfg(target_os = "macos")]
        user.push("Library/Audio/Plug-Ins/CLAP");
        #[cfg(target_os = "linux")]
        user.push(".clap");
        #[cfg(target_os = "windows")]
        user.push("AppData/Local/Programs/Common/CLAP");
        out.push(user);
    }
    #[cfg(target_os = "macos")]
    out.push(PathBuf::from("/Library/Audio/Plug-Ins/CLAP"));
    #[cfg(target_os = "linux")]
    out.push(PathBuf::from("/usr/lib/clap"));
    #[cfg(target_os = "windows")]
    {
        if let Some(pf) = std::env::var_os("CommonProgramFiles") {
            let mut p = PathBuf::from(pf);
            p.push("CLAP");
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
        if !name.ends_with(CLAP_EXTENSION) {
            continue;
        }
        // Failure to scan one bundle should not abort the whole
        // walk — the host still wants the plugins it can see.
        if let Err(err) = scan_bundle_into(&path, out) {
            tracing::debug!(path = %path.display(), error = %err, "skipping CLAP plugin");
        }
    }
}

fn scan_bundle_into(bundle_path: &Path, out: &mut Vec<PluginInfo>) -> Result<()> {
    let binary = bundle_binary_path(bundle_path);
    let handle = unsafe { LoadedLibrary::open(&binary)? };
    let entry = handle.entry()?;
    unsafe { entry.init(&binary)? };
    let factory = unsafe { entry.factory() };
    if !factory.is_null() {
        let count = unsafe { ((*factory).get_plugin_count.unwrap_or(empty_count))(factory) };
        for idx in 0..count {
            let desc =
                unsafe { ((*factory).get_plugin_descriptor.unwrap_or(empty_desc))(factory, idx) };
            if desc.is_null() {
                continue;
            }
            out.push(unsafe { descriptor_to_info(bundle_path, &*desc) });
        }
    }
    unsafe { entry.deinit() };
    Ok(())
}

unsafe extern "C" fn empty_count(_: *const clap_plugin_factory) -> u32 {
    0
}

unsafe extern "C" fn empty_desc(
    _: *const clap_plugin_factory,
    _: u32,
) -> *const clap_plugin_descriptor {
    ptr::null()
}

/// Per-platform `.clap` bundle layout. macOS uses NSBundle-style
/// `Contents/MacOS/<stem>`; Linux / Windows treat the `.clap`
/// file itself as the dylib.
pub(crate) fn bundle_binary_path(bundle: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        let stem = bundle.file_stem().unwrap_or_default();
        if bundle.is_dir() {
            return bundle.join("Contents/MacOS").join(stem);
        }
    }
    bundle.to_path_buf()
}

unsafe fn descriptor_to_info(bundle_path: &Path, desc: &clap_plugin_descriptor) -> PluginInfo {
    let id = unsafe { cstr_to_string(desc.id) };
    let name = unsafe { cstr_to_string(desc.name) };
    let vendor = unsafe { cstr_to_string(desc.vendor) };
    let version_str = unsafe { cstr_to_string(desc.version) };
    let version = parse_version(&version_str);
    let (category, accepts_midi) = unsafe { categorize(desc.features) };
    let supports_ara = unsafe { features_support_ara(desc.features) };
    PluginInfo {
        name,
        vendor,
        version,
        category,
        path: bundle_path.to_path_buf(),
        unique_id: id,
        format: FORMAT,
        has_editor: false, // honest default — set during load via the GUI ext.
        accepts_midi,
        supports_ara,
    }
}

/// CLAP feature strings live in a NULL-terminated array of C
/// strings. We scan for instrument / note-effect / analyzer
/// markers and the `note-input` / `note-effect` markers used to
/// flag MIDI capability.
unsafe fn categorize(features: *const *const c_char) -> (PluginCategory, bool) {
    if features.is_null() {
        return (PluginCategory::Effect, false);
    }
    let mut category = PluginCategory::Effect;
    let mut accepts_midi = false;
    let mut idx = 0usize;
    loop {
        let p = unsafe { *features.add(idx) };
        if p.is_null() {
            break;
        }
        let bytes = unsafe { CStr::from_ptr(p).to_bytes() };
        match bytes {
            b"instrument" => category = PluginCategory::Instrument,
            b"note-effect" => {
                category = PluginCategory::NoteEffect;
                accepts_midi = true;
            }
            b"analyzer" => category = PluginCategory::Analyzer,
            b"utility" => category = PluginCategory::Tool,
            b"note-input" => accepts_midi = true,
            _ => {}
        }
        idx += 1;
    }
    (category, accepts_midi)
}

/// Returns `true` when the CLAP feature list advertises ARA support.
unsafe fn features_support_ara(features: *const *const c_char) -> bool {
    if features.is_null() {
        return false;
    }
    let mut index = 0usize;
    loop {
        let feature = unsafe { *features.add(index) };
        if feature.is_null() {
            return false;
        }
        let bytes = unsafe { CStr::from_ptr(feature) }.to_bytes();
        if bytes == b"ara:supported" || bytes == b"ara:required" {
            return true;
        }
        index += 1;
    }
}

/// CLAP versions are `"major.minor.patch"`. We pack the first
/// three numeric components into a `u32` as
/// `(major << 16) | (minor << 8) | patch`, matching what the
/// legacy rack used.
fn parse_version(s: &str) -> u32 {
    let mut parts = s.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
    let major = parts.next().unwrap_or(0);
    let minor = parts.next().unwrap_or(0);
    let patch = parts.next().unwrap_or(0);
    (major << 16) | (minor << 8) | patch
}

unsafe fn cstr_to_string(p: *const c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

/// RAII wrapper around a `libloading::Library` plus the
/// `clap_entry` symbol it exports.
pub(crate) struct LoadedLibrary {
    library: libloading::Library,
}

impl LoadedLibrary {
    pub(crate) unsafe fn open(path: &Path) -> Result<Self> {
        // SAFETY: caller guarantees `path` points at a valid
        // CLAP plugin bundle binary; `libloading::Library::new`
        // is itself the OS dlopen call.
        let library = unsafe { libloading::Library::new(path) }.map_err(|e| Error::LoadFailed {
            path: path.to_path_buf(),
            reason: format!("dlopen failed: {e}"),
        })?;
        Ok(Self { library })
    }

    pub(crate) fn entry(&self) -> Result<EntryRef<'_>> {
        let symbol: libloading::Symbol<'_, *const clap_plugin_entry> = unsafe {
            self.library
                .get(ENTRY_SYMBOL)
                .map_err(|e| Error::LoadFailed {
                    path: PathBuf::new(),
                    reason: format!("missing clap_entry symbol: {e}"),
                })?
        };
        let entry = *symbol;
        if entry.is_null() {
            return Err(Error::LoadFailed {
                path: PathBuf::new(),
                reason: "clap_entry symbol resolved to NULL".into(),
            });
        }
        Ok(EntryRef {
            entry,
            _phantom: std::marker::PhantomData,
        })
    }
}

pub(crate) struct EntryRef<'a> {
    entry: *const clap_plugin_entry,
    // The reference lifetime ties the entry pointer to the
    // library handle it was loaded from.
    #[allow(dead_code)]
    _phantom: std::marker::PhantomData<&'a LoadedLibrary>,
}

impl EntryRef<'_> {
    pub(crate) unsafe fn init(&self, path: &Path) -> Result<()> {
        let init = unsafe { (*self.entry).init };
        if let Some(init) = init {
            let c_path =
                CString::new(path.to_string_lossy().as_bytes()).map_err(|e| Error::LoadFailed {
                    path: path.to_path_buf(),
                    reason: format!("plugin path is not a valid C string: {e}"),
                })?;
            if !unsafe { init(c_path.as_ptr()) } {
                return Err(Error::LoadFailed {
                    path: path.to_path_buf(),
                    reason: "clap_entry::init returned false".into(),
                });
            }
        }
        Ok(())
    }

    pub(crate) unsafe fn deinit(&self) {
        if let Some(deinit) = unsafe { (*self.entry).deinit } {
            unsafe { deinit() };
        }
    }

    pub(crate) unsafe fn factory(&self) -> *const clap_plugin_factory {
        let Some(get_factory) = (unsafe { (*self.entry).get_factory }) else {
            return ptr::null();
        };
        let raw = unsafe { get_factory(CLAP_PLUGIN_FACTORY_ID.as_ptr()) };
        raw.cast::<clap_plugin_factory>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_version_components() {
        assert_eq!(parse_version("1.2.3"), (1 << 16) | (2 << 8) | 3);
        assert_eq!(parse_version("0.5"), 5 << 8);
        assert_eq!(parse_version(""), 0);
    }

    #[test]
    fn bundle_binary_macos() {
        let p = bundle_binary_path(Path::new("/tmp/MyPlugin.clap"));
        #[cfg(target_os = "macos")]
        assert!(!p.exists() || p.starts_with("/tmp/MyPlugin.clap"));
        #[cfg(not(target_os = "macos"))]
        assert_eq!(p, Path::new("/tmp/MyPlugin.clap"));
    }
}

#[cfg(test)]
mod ara_tests {
    use super::features_support_ara;
    use std::ffi::c_char;

    #[test]
    fn detects_ara_features() {
        static SUPPORTED: &[u8] = b"ara:supported\0";
        static REQUIRED: &[u8] = b"ara:required\0";
        static OTHER: &[u8] = b"instrument\0";
        let features = [
            OTHER.as_ptr().cast::<c_char>(),
            SUPPORTED.as_ptr().cast::<c_char>(),
            std::ptr::null(),
        ];
        assert!(unsafe { features_support_ara(features.as_ptr()) });
        let required = [REQUIRED.as_ptr().cast::<c_char>(), std::ptr::null()];
        assert!(unsafe { features_support_ara(required.as_ptr()) });
        let plain = [OTHER.as_ptr().cast::<c_char>(), std::ptr::null()];
        assert!(!unsafe { features_support_ara(plain.as_ptr()) });
    }
}
