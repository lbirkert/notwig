//! VST 2.4 host implementation for the notwig-rack framework.
//!
//! VST 2 predates the modern plugin APIs and is a plain C ABI: a
//! plugin exports `VSTPluginMain`, returns an `AEffect` struct of
//! function pointers, and talks back to the host through a single
//! callback. This crate wraps that contract behind the
//! format-agnostic [`notwig_rack_core`] traits.
//!
//! The ABI declarations live in [`abi`] and are translated from
//! Xaymar's clean-room `vst2sdk` headers (BSD-3-Clause). See the
//! repository `NOTICE` file for the full provenance.
//!
//! VST 2 audio is always 32-bit float. Nodes are planar on the host
//! side and `processReplacing` is preferred when available.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::missing_safety_doc
)]

mod abi;
mod loader;

use abi::{AEffect, EventList as VstEventList, MAX_EVENTS, MidiEvent, Rect, SysExEvent, TimeInfo};
use crossbeam_queue::ArrayQueue;
use loader::LoadedModule;
use notwig_rack_core::buffer::AudioBuffer;
use notwig_rack_core::bus::{Bus, BusLayout, ChannelConfig};
use notwig_rack_core::editor::{PluginEditor, WindowHandle};
use notwig_rack_core::error::{Error, Result};
use notwig_rack_core::events::{Event, EventBody, EventList, MidiData};
use notwig_rack_core::info::{
    ParameterFlags, ParameterInfo, PluginCategory, PluginInfo, PresetInfo,
};
use notwig_rack_core::plugin::{Plugin, PluginCore, ProcessContext, ProcessStatus};
use notwig_rack_core::scanner::PluginScanner;
use notwig_rack_core::transport::TransportInfo;
use notwig_rack_core::wrapper::run_audio_block_with;

use std::cell::{Cell, UnsafeCell};
use std::ffi::{CString, OsStr, c_void};
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};

/// Format identifier used on returned [`PluginInfo`] entries.
pub const FORMAT: &str = "vst2";

/// Filename extension used by VST 2 plugins on every platform.
pub const VST2_EXTENSION: &str = ".vst";

const DEFAULT_SAMPLE_RATE: f64 = 48_000.0;
const DEFAULT_BLOCK_SIZE: usize = 512;
const MAX_CHANNELS: usize = 64;
const MAX_STATE_BYTES: usize = 64 * 1024 * 1024;

thread_local! {
    static CURRENT_ID: Cell<i32> = const { Cell::new(0) };
    static PROCESS_LEVEL: Cell<isize> = const { Cell::new(1) };
    static TIME: UnsafeCell<TimeInfo> = const { UnsafeCell::new(TimeInfo::ZERO) };
}

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
        Err(err) => eprintln!("[notwig-rack-vst2] skipping {}: {err}", path.display()),
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

fn resolve_binary(path: &Path) -> PathBuf {
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

fn plugin_directory(path: &Path) -> Option<CString> {
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

/// One loaded VST 2 plugin instance.
pub struct Vst2Plugin {
    info: PluginInfo,
    layouts: Vec<BusLayout>,
    active_layout: Option<BusLayout>,
    #[allow(dead_code)]
    module: LoadedModule,
    effect: *mut AEffect,
    host: Box<HostShared>,
    parameter_count: usize,
    num_inputs: usize,
    num_outputs: usize,
    sample_rate: f64,
    max_block_size: usize,
    input_silence: Vec<Vec<f32>>,
    output_scratch: Vec<Vec<f32>>,
    active: bool,
    editor_open: bool,
    editor_size: Cell<Option<(u32, u32)>>,
}

// SAFETY: a plugin instance is owned by one thread at a time (the
// `Plugin` trait takes `&mut self`), and the loaded module stays
// mapped for the instance's lifetime. The raw AEffect pointer is
// never shared without that exclusive borrow.
unsafe impl Send for Vst2Plugin {}

impl Vst2Plugin {
    fn load_path(path: &Path, child_id: i32) -> Result<Self> {
        let info = PluginInfo {
            name: String::new(),
            vendor: String::new(),
            version: 0,
            category: PluginCategory::Effect,
            path: path.to_path_buf(),
            unique_id: child_id.to_string(),
            format: FORMAT,
            has_editor: false,
            accepts_midi: false,
        };
        Self::load_from(&info)
    }

    #[allow(clippy::too_many_lines)]
    fn load_from(info: &PluginInfo) -> Result<Self> {
        let binary = resolve_binary(&info.path);
        let module = unsafe { LoadedModule::open(&info.path, &binary) }?;

        let child_id = info.unique_id.parse::<i32>().unwrap_or(0);
        let host = Box::new(HostShared::new(plugin_directory(&info.path)));
        let entry = unsafe { module.entry() }?;
        let effect = {
            let previous = CURRENT_ID.replace(child_id);
            let effect = unsafe { entry(host_callback) };
            CURRENT_ID.set(previous);
            effect
        };

        if effect.is_null() {
            return Err(Error::LoadFailed {
                path: info.path.clone(),
                reason: "VSTPluginMain returned null".into(),
            });
        }

        let fields = unsafe { effect.read() };
        if let Err(err) = validate_effect(&fields, &info.path) {
            if let Some(close) = fields.dispatcher {
                unsafe { close(effect, abi::EFF_CLOSE, 0, 0, ptr::null_mut(), 0.0) };
            }
            return Err(err);
        }

        // resvd1 is host-owned storage; the plugin must not touch it.
        unsafe {
            let host_ptr = (&raw const *host).cast_mut().cast::<c_void>();
            (*effect).resvd1 = host_ptr;
        }

        let _ = dispatch(effect, abi::EFF_OPEN, 0, 0, ptr::null_mut(), 0.0);
        let fields = unsafe { effect.read() };
        if let Err(err) = validate_effect(&fields, &info.path) {
            if let Some(close) = fields.dispatcher {
                unsafe { close(effect, abi::EFF_CLOSE, 0, 0, ptr::null_mut(), 0.0) };
            }
            return Err(err);
        }
        if child_id != 0 && fields.unique_id != 0 && fields.unique_id != child_id {
            if let Some(close) = fields.dispatcher {
                unsafe { close(effect, abi::EFF_CLOSE, 0, 0, ptr::null_mut(), 0.0) };
            }
            return Err(Error::LoadFailed {
                path: info.path.clone(),
                reason: format!(
                    "VST2 shell requested id {child_id}, module reported {}",
                    fields.unique_id
                ),
            });
        }

        let parameter_count = usize::try_from(fields.num_params).unwrap_or(0);
        let num_inputs = usize::try_from(fields.num_inputs).unwrap_or(0);
        let num_outputs = usize::try_from(fields.num_outputs).unwrap_or(0);

        let mut info = info.clone();
        if info.name.is_empty() {
            info.name = dispatch_text(effect, abi::EFF_GET_EFFECT_NAME, 0);
        }
        if info.vendor.is_empty() {
            info.vendor = dispatch_text(effect, abi::EFF_GET_VENDOR_STRING, 0);
        }
        if info.unique_id.is_empty() || info.unique_id == "0" {
            info.unique_id = fields.unique_id.to_string();
        }
        info.version = u32::try_from(fields.version).unwrap_or(0);
        info.category = effect_category(effect, &fields);
        info.has_editor = fields.flags & abi::FLAG_HAS_EDITOR != 0;
        info.accepts_midi = fields.flags & abi::FLAG_IS_SYNTH != 0
            || dispatch(
                effect,
                abi::EFF_CAN_DO,
                0,
                0,
                c"receiveVstMidiEvent".as_ptr().cast_mut().cast(),
                0.0,
            ) == 1;
        info.format = FORMAT;

        Ok(Self {
            info,
            layouts: vec![layout_for(num_inputs, num_outputs)],
            active_layout: None,
            module,
            effect,
            host,
            parameter_count,
            num_inputs,
            num_outputs,
            sample_rate: DEFAULT_SAMPLE_RATE,
            max_block_size: DEFAULT_BLOCK_SIZE,
            input_silence: Vec::new(),
            output_scratch: Vec::new(),
            active: false,
            editor_open: false,
            editor_size: Cell::new(None),
        })
    }

    fn class_infos(&self) -> Vec<PluginInfo> {
        let category = dispatch(
            self.effect,
            abi::EFF_GET_PLUG_CATEGORY,
            0,
            0,
            ptr::null_mut(),
            0.0,
        );
        if category as i32 != abi::PLUG_CATEG_SHELL {
            return vec![self.info.clone()];
        }

        let mut out = Vec::new();
        for _ in 0..1024 {
            let mut name = [0u8; 256];
            let id = dispatch(
                self.effect,
                abi::EFF_SHELL_GET_NEXT_PLUGIN,
                0,
                0,
                name.as_mut_ptr().cast(),
                0.0,
            ) as i32;
            if id == 0 {
                break;
            }
            let unique_id = id.to_string();
            if out
                .iter()
                .any(|entry: &PluginInfo| entry.unique_id == unique_id)
            {
                break;
            }
            let end = name
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(name.len());
            out.push(PluginInfo {
                name: String::from_utf8_lossy(&name[..end]).into_owned(),
                vendor: self.info.vendor.clone(),
                version: self.info.version,
                category: PluginCategory::Effect,
                path: self.info.path.clone(),
                unique_id,
                format: FORMAT,
                has_editor: self.info.has_editor,
                accepts_midi: self.info.accepts_midi,
            });
        }

        if out.is_empty() {
            vec![self.info.clone()]
        } else {
            out
        }
    }

    fn dispatch(
        &self,
        opcode: i32,
        index: i32,
        value: isize,
        pointer: *mut c_void,
        option: f32,
    ) -> isize {
        dispatch(self.effect, opcode, index, value, pointer, option)
    }

    fn text(&self, opcode: i32, index: i32) -> String {
        dispatch_text(self.effect, opcode, index)
    }

    fn query_editor_size(&self) -> Option<(u32, u32)> {
        let mut rect: *mut Rect = ptr::null_mut();
        self.dispatch(abi::EFF_EDIT_GET_RECT, 0, 0, (&raw mut rect).cast(), 0.0);
        if rect.is_null() {
            return None;
        }
        rect_size(unsafe { rect.read() })
    }
}

fn rect_size(rect: Rect) -> Option<(u32, u32)> {
    let width = i32::from(rect.right).saturating_sub(i32::from(rect.left));
    let height = i32::from(rect.bottom).saturating_sub(i32::from(rect.top));
    if width <= 0 || height <= 0 {
        return None;
    }
    Some((u32::try_from(width).ok()?, u32::try_from(height).ok()?))
}

impl Drop for Vst2Plugin {
    fn drop(&mut self) {
        if self.editor_open {
            self.close();
        }
        if self.active {
            self.deactivate();
        }
        if let Some(close) = unsafe { (*self.effect).dispatcher } {
            unsafe { close(self.effect, abi::EFF_CLOSE, 0, 0, ptr::null_mut(), 0.0) };
        }
    }
}

fn validate_effect(fields: &AEffect, path: &Path) -> Result<()> {
    if fields.magic != abi::MAGIC || fields.dispatcher.is_none() {
        return Err(Error::LoadFailed {
            path: path.to_path_buf(),
            reason: "invalid VST2 AEffect header".into(),
        });
    }
    if !(0..=64).contains(&fields.num_inputs)
        || !(0..=64).contains(&fields.num_outputs)
        || !(0..=16384).contains(&fields.num_params)
    {
        return Err(Error::LoadFailed {
            path: path.to_path_buf(),
            reason: "VST2 channel or parameter count out of range".into(),
        });
    }
    if fields.process_replacing.is_none() && fields.process.is_none() {
        return Err(Error::LoadFailed {
            path: path.to_path_buf(),
            reason: "VST2 plugin exposes no 32-bit process callback".into(),
        });
    }
    Ok(())
}

fn effect_category(effect: *mut AEffect, fields: &AEffect) -> PluginCategory {
    if fields.flags & abi::FLAG_IS_SYNTH != 0 {
        return PluginCategory::Instrument;
    }
    match dispatch(
        effect,
        abi::EFF_GET_PLUG_CATEGORY,
        0,
        0,
        ptr::null_mut(),
        0.0,
    ) as i32
    {
        abi::PLUG_CATEG_SYNTH => PluginCategory::Instrument,
        abi::PLUG_CATEG_EFFECT => PluginCategory::Effect,
        _ => {
            if fields.flags & abi::FLAG_IS_EFFECT != 0 {
                PluginCategory::Effect
            } else {
                PluginCategory::Tool
            }
        }
    }
}

fn channel_config(channels: usize) -> ChannelConfig {
    match channels {
        0 => ChannelConfig::Discrete(0),
        1 => ChannelConfig::Mono,
        2 => ChannelConfig::Stereo,
        6 => ChannelConfig::Surround5_1,
        8 => ChannelConfig::Surround7_1,
        other => ChannelConfig::Discrete(u32::try_from(other).unwrap_or(u32::MAX)),
    }
}

fn layout_for(inputs: usize, outputs: usize) -> BusLayout {
    let mut layout = BusLayout::new();
    if inputs > 0 {
        layout
            .inputs
            .push(Bus::main("Input", channel_config(inputs)));
    }
    if outputs > 0 {
        layout
            .outputs
            .push(Bus::main("Output", channel_config(outputs)));
    }
    layout
}

impl PluginCore for Vst2Plugin {
    fn info(&self) -> &PluginInfo {
        &self.info
    }

    fn active_layout(&self) -> Option<&BusLayout> {
        self.active_layout.as_ref()
    }

    fn supported_layouts(&self) -> &[BusLayout] {
        &self.layouts
    }

    fn parameter_count(&self) -> usize {
        self.parameter_count
    }

    fn parameter_info(&self, index: usize) -> Result<ParameterInfo> {
        if index >= self.parameter_count {
            return Err(Error::InvalidParameter(index));
        }
        let index_i32 = i32::try_from(index).map_err(|_| Error::InvalidParameter(index))?;
        let name = self.text(abi::EFF_GET_PARAM_NAME, index_i32);
        let unit = self.text(abi::EFF_GET_PARAM_LABEL, index_i32);
        let default = unsafe {
            (*self.effect)
                .get_parameter
                .map_or(0.0, |get| f64::from(get(self.effect, index as u32)))
        };
        let mut flags = ParameterFlags::empty();
        if unsafe { (*self.effect).set_parameter.is_none() } {
            flags |= ParameterFlags::READ_ONLY;
        }
        if self.dispatch(
            abi::EFF_CAN_BE_AUTOMATED,
            index_i32,
            0,
            ptr::null_mut(),
            0.0,
        ) != 0
        {
            flags |= ParameterFlags::AUTOMATABLE;
        }
        Ok(ParameterInfo {
            id: u32::try_from(index).unwrap_or(u32::MAX),
            name: name.clone(),
            short_name: name,
            unit,
            min: 0.0,
            max: 1.0,
            default,
            step_count: 0,
            flags,
        })
    }

    fn parameter_value(&self, index: usize) -> Result<f64> {
        if index >= self.parameter_count {
            return Err(Error::InvalidParameter(index));
        }
        let get = unsafe { (*self.effect).get_parameter }
            .ok_or_else(|| Error::Other("VST2 plugin has no getParameter".into()))?;
        Ok(f64::from(unsafe { get(self.effect, index as u32) }))
    }

    fn parameter_value_string(&self, index: usize, value: f64) -> Result<String> {
        if index >= self.parameter_count {
            return Err(Error::InvalidParameter(index));
        }
        let text = self.text(
            abi::EFF_GET_PARAM_DISPLAY,
            i32::try_from(index).unwrap_or(0),
        );
        if text.is_empty() {
            Ok(format!("{value:.3}"))
        } else {
            Ok(text)
        }
    }

    fn set_parameter(&mut self, index: usize, value: f64) -> Result<()> {
        if index >= self.parameter_count {
            return Err(Error::InvalidParameter(index));
        }
        let set = unsafe { (*self.effect).set_parameter }
            .ok_or_else(|| Error::Other("VST2 plugin has no setParameter".into()))?;
        let value = value.clamp(0.0, 1.0) as f32;
        unsafe { set(self.effect, index as u32, value) };
        Ok(())
    }

    fn preset_count(&self) -> usize {
        usize::try_from(unsafe { (*self.effect).num_programs }).unwrap_or(0)
    }

    fn preset_info(&self, index: usize) -> Result<PresetInfo> {
        if index >= self.preset_count() {
            return Err(Error::InvalidParameter(index));
        }
        let index_i32 = i32::try_from(index).map_err(|_| Error::InvalidParameter(index))?;
        let mut name = self.text(abi::EFF_GET_PROGRAM_NAME_INDEXED, index_i32);
        if name.is_empty() {
            name = self.text(abi::EFF_GET_PROGRAM_NAME, 0);
        }
        Ok(PresetInfo {
            index,
            name,
            preset_number: index_i32,
        })
    }

    fn load_preset(&mut self, preset_number: i32) -> Result<()> {
        if preset_number < 0
            || usize::try_from(preset_number).unwrap_or(usize::MAX) >= self.preset_count()
        {
            return Err(Error::InvalidParameter(
                usize::try_from(preset_number).unwrap_or(usize::MAX),
            ));
        }
        self.dispatch(
            abi::EFF_SET_PROGRAM,
            0,
            preset_number as isize,
            ptr::null_mut(),
            0.0,
        );
        Ok(())
    }

    fn save_state(&self) -> Result<Vec<u8>> {
        let fields = unsafe { self.effect.read() };
        if fields.flags & abi::FLAG_PROGRAM_CHUNKS != 0 {
            let mut data: *const u8 = ptr::null();
            let size = self.dispatch(abi::EFF_GET_CHUNK, 0, 0, (&raw mut data).cast(), 0.0);
            if size < 0 || usize::try_from(size).unwrap_or(usize::MAX) > MAX_STATE_BYTES {
                return Err(Error::Other(
                    "VST2 effGetChunk returned an invalid size".into(),
                ));
            }
            if size == 0 {
                return Ok(Vec::new());
            }
            if data.is_null() {
                return Err(Error::Other(
                    "VST2 effGetChunk returned a null pointer".into(),
                ));
            }
            return Ok(unsafe { std::slice::from_raw_parts(data, size as usize) }.to_vec());
        }

        let program = if fields.num_programs > 0 {
            self.dispatch(abi::EFF_GET_PROGRAM, 0, 0, ptr::null_mut(), 0.0) as i32
        } else {
            0
        };
        let get = fields
            .get_parameter
            .ok_or_else(|| Error::Other("VST2 plugin has no getParameter".into()))?;
        let mut out = Vec::with_capacity(4 + self.parameter_count * 4);
        out.extend_from_slice(&program.to_le_bytes());
        for index in 0..self.parameter_count {
            let value = unsafe { get(self.effect, index as u32) };
            out.extend_from_slice(&value.to_le_bytes());
        }
        Ok(out)
    }

    fn load_state(&mut self, bytes: &[u8]) -> Result<()> {
        if bytes.len() > MAX_STATE_BYTES {
            return Err(Error::Other("VST2 state blob too large".into()));
        }
        let fields = unsafe { self.effect.read() };
        if fields.flags & abi::FLAG_PROGRAM_CHUNKS != 0 {
            let mut blob = bytes.to_vec();
            self.dispatch(
                abi::EFF_SET_CHUNK,
                0,
                blob.len() as isize,
                blob.as_mut_ptr().cast(),
                0.0,
            );
            return Ok(());
        }

        let expected = 4 + self.parameter_count * 4;
        if bytes.len() != expected {
            return Err(Error::Other(format!(
                "VST2 parameter state has {} bytes, expected {expected}",
                bytes.len()
            )));
        }
        let program =
            i32::from_le_bytes(bytes[..4].try_into().map_err(|_| {
                Error::Other("VST2 parameter state has a short program number".into())
            })?);
        if fields.num_programs > 0 {
            if program < 0 || program >= fields.num_programs {
                return Err(Error::Other(
                    "VST2 parameter state has an invalid program".into(),
                ));
            }
            self.dispatch(
                abi::EFF_SET_PROGRAM,
                0,
                program as isize,
                ptr::null_mut(),
                0.0,
            );
        }

        let set = fields
            .set_parameter
            .ok_or_else(|| Error::Other("VST2 plugin has no setParameter".into()))?;
        for (index, chunk) in bytes[4..].as_chunks::<4>().0.iter().enumerate() {
            let value = f32::from_le_bytes(*chunk);
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err(Error::Other(
                    "VST2 parameter state contains an invalid value".into(),
                ));
            }
            unsafe { set(self.effect, index as u32, value) };
        }
        Ok(())
    }

    fn activate(
        &mut self,
        layout: BusLayout,
        sample_rate: f64,
        max_block_size: usize,
    ) -> Result<()> {
        if self.active {
            self.deactivate();
        }
        self.dispatch(abi::EFF_SET_PROCESS_PRECISION, 0, 0, ptr::null_mut(), 0.0);
        self.dispatch(
            abi::EFF_SET_SAMPLE_RATE,
            0,
            0,
            ptr::null_mut(),
            sample_rate as f32,
        );
        self.dispatch(
            abi::EFF_SET_BLOCK_SIZE,
            0,
            max_block_size as isize,
            ptr::null_mut(),
            0.0,
        );
        self.dispatch(abi::EFF_MAINS_CHANGED, 0, 1, ptr::null_mut(), 0.0);
        self.dispatch(abi::EFF_START_PROCESS, 0, 0, ptr::null_mut(), 0.0);

        self.sample_rate = sample_rate;
        self.max_block_size = max_block_size;
        let block = max_block_size.max(1);
        self.input_silence = (0..self.num_inputs).map(|_| vec![0.0; block]).collect();
        self.output_scratch = (0..self.num_outputs).map(|_| vec![0.0; block]).collect();
        self.host
            .sample_rate
            .store(sample_rate.to_bits(), Ordering::Relaxed);
        self.host
            .block_size
            .store(max_block_size, Ordering::Relaxed);
        self.active = true;
        self.active_layout = Some(layout);
        Ok(())
    }

    fn deactivate(&mut self) {
        if !self.active {
            return;
        }
        self.dispatch(abi::EFF_STOP_PROCESS, 0, 0, ptr::null_mut(), 0.0);
        self.dispatch(abi::EFF_MAINS_CHANGED, 0, 0, ptr::null_mut(), 0.0);
        self.active = false;
        self.active_layout = None;
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn latency_samples(&self) -> u32 {
        let delay = unsafe { (*self.effect).initial_delay };
        u32::try_from(delay.max(0)).unwrap_or(0)
    }

    fn editor(&mut self) -> Option<&mut dyn PluginEditor> {
        if self.info.has_editor {
            Some(self)
        } else {
            None
        }
    }
}

impl Plugin<f32> for Vst2Plugin {
    #[allow(clippy::too_many_lines)]
    fn process(
        &mut self,
        buffer: &mut AudioBuffer<'_, f32>,
        events: &EventList,
        context: &mut ProcessContext<'_>,
    ) -> Result<ProcessStatus> {
        if !self.active {
            return Err(Error::NotActivated);
        }
        let frames = buffer.num_frames();
        if frames == 0 {
            return Ok(ProcessStatus::Continue);
        }

        self.host
            .sample_rate
            .store(context.sample_rate.to_bits(), Ordering::Relaxed);
        self.host
            .block_size
            .store(self.max_block_size, Ordering::Relaxed);
        self.host.time.update(context.transport);

        let mut input_ptrs = [ptr::null::<f32>(); MAX_CHANNELS];
        {
            let inputs = buffer.main_inputs();
            let provided = inputs.len().min(self.num_inputs);
            for (slot, channel) in input_ptrs.iter_mut().zip(inputs.iter()).take(provided) {
                *slot = channel.as_ptr();
            }
            for (index, slot) in input_ptrs
                .iter_mut()
                .enumerate()
                .take(self.num_inputs)
                .skip(provided)
            {
                let Some(channel) = self.input_silence.get(index) else {
                    return Err(Error::Other("VST2 input scratch is not allocated".into()));
                };
                if channel.len() < frames {
                    return Err(Error::Other("VST2 input scratch is too small".into()));
                }
                *slot = channel.as_ptr();
            }
        }

        let mut output_ptrs = [ptr::null_mut::<f32>(); MAX_CHANNELS];
        {
            let outputs = buffer.main_outputs();
            let provided = outputs.len().min(self.num_outputs);
            for (slot, channel) in output_ptrs
                .iter_mut()
                .zip(outputs.iter_mut())
                .take(provided)
            {
                *slot = channel.as_mut_ptr();
            }
            for (index, slot) in output_ptrs
                .iter_mut()
                .enumerate()
                .take(self.num_outputs)
                .skip(provided)
            {
                let Some(channel) = self.output_scratch.get_mut(index) else {
                    return Err(Error::Other("VST2 output scratch is not allocated".into()));
                };
                if channel.len() < frames {
                    return Err(Error::Other("VST2 output scratch is too small".into()));
                }
                *slot = channel.as_mut_ptr();
            }
        }

        let mut midi_events = [MidiEvent::default(); MAX_EVENTS];
        let mut sysex_events = [SysExEvent::default(); MAX_EVENTS];
        let mut sysex_bytes = [[0u8; 8]; MAX_EVENTS];
        let mut list = VstEventList::default();

        for event in events {
            match event.body {
                EventBody::Midi(midi) => {
                    if let Some(raw) = midi_to_bytes(midi) {
                        if raw.len <= 3 {
                            push_midi_event(
                                &mut list,
                                &mut midi_events,
                                event.sample_offset,
                                &raw.data,
                            );
                        } else {
                            push_sysex_event(
                                &mut list,
                                &mut sysex_events,
                                &mut sysex_bytes,
                                event.sample_offset,
                                &raw.data[..usize::from(raw.len)],
                            );
                        }
                    }
                }
                EventBody::ParamValue { param_id, value } => {
                    if let Some(set) = unsafe { (*self.effect).set_parameter }
                        && (param_id as usize) < self.parameter_count
                    {
                        unsafe { set(self.effect, param_id, value.clamp(0.0, 1.0) as f32) };
                    }
                }
                EventBody::ParamGesture { .. } | EventBody::TransportFlag(_) => {}
            }
        }

        if list.num_events > 0 {
            let _ = self.dispatch(abi::EFF_PROCESS_EVENTS, 0, 0, (&raw mut list).cast(), 0.0);
        }

        let _level = ProcessLevel::enter();
        let effect = self.effect;
        let num_inputs = self.num_inputs;
        let num_outputs = self.num_outputs;
        let ok = run_audio_block_with::<Vst2Plugin, bool>(FORMAT, false, || unsafe {
            if let Some(process) = (*effect).process_replacing {
                process(
                    effect,
                    input_ptrs.as_ptr(),
                    output_ptrs.as_mut_ptr(),
                    frames as i32,
                );
                true
            } else if let Some(process) = (*effect).process {
                for index in 0..num_outputs {
                    if index < num_inputs && !input_ptrs[index].is_null() {
                        ptr::copy_nonoverlapping(input_ptrs[index], output_ptrs[index], frames);
                    } else if !output_ptrs[index].is_null() {
                        ptr::write_bytes(output_ptrs[index], 0, frames);
                    }
                }
                process(
                    effect,
                    input_ptrs.as_ptr(),
                    output_ptrs.as_mut_ptr(),
                    frames as i32,
                );
                true
            } else {
                false
            }
        });

        while let Some(event) = self.host.output_events.pop() {
            context.output_events.push(event);
        }

        if ok {
            Ok(ProcessStatus::Continue)
        } else {
            for pointer in output_ptrs.iter().take(self.num_outputs) {
                if !pointer.is_null() {
                    unsafe { ptr::write_bytes(*pointer, 0, frames) };
                }
            }
            Ok(ProcessStatus::Error)
        }
    }
}

impl PluginEditor for Vst2Plugin {
    fn open(&mut self, parent: WindowHandle, _scale: f64) -> Result<()> {
        if self.editor_open {
            return Ok(());
        }
        let handle = match parent {
            WindowHandle::NSView(pointer) | WindowHandle::HWND(pointer) => pointer,
            WindowHandle::X11(id) => id as usize as *mut c_void,
        };
        let opened = self.dispatch(abi::EFF_EDIT_OPEN, 0, 0, handle, 0.0);
        let size = self.query_editor_size();
        if opened == 0 && size.is_none() {
            self.dispatch(abi::EFF_EDIT_CLOSE, 0, 0, ptr::null_mut(), 0.0);
            return Err(Error::Other("VST2 effEditOpen failed".into()));
        }
        self.editor_open = true;
        self.host.editor_open.store(true, Ordering::Release);
        self.editor_size.set(size);
        Ok(())
    }

    fn close(&mut self) {
        if !self.editor_open {
            return;
        }
        self.dispatch(abi::EFF_EDIT_CLOSE, 0, 0, ptr::null_mut(), 0.0);
        self.editor_open = false;
        self.host.editor_open.store(false, Ordering::Release);
        self.host.editor_size.store(0, Ordering::Release);
        self.editor_size.set(None);
    }

    fn is_open(&self) -> bool {
        self.editor_open
    }

    fn size(&self) -> Option<(u32, u32)> {
        if let Some(size) = self.host.take_editor_size() {
            self.editor_size.set(Some(size));
        }
        if let Some(size) = self.editor_size.get() {
            return Some(size);
        }
        let size = self.query_editor_size();
        self.editor_size.set(size);
        size
    }

    fn is_resizable(&self) -> bool {
        false
    }

    fn set_size(&mut self, _width: u32, _height: u32) -> Option<(u32, u32)> {
        // VST 2 has no standard host-driven resize request.
        None
    }

    fn show(&mut self) {}

    fn hide(&mut self) {}

    fn on_idle(&mut self) {
        if !self.editor_open {
            return;
        }
        self.dispatch(abi::EFF_EDIT_IDLE, 0, 0, ptr::null_mut(), 0.0);
        if let Some(size) = self.host.take_editor_size() {
            self.editor_size.set(Some(size));
        }
    }
}

struct TimeState {
    sample_position: AtomicU64,
    ppq_position: AtomicU64,
    tempo: AtomicU64,
    bar_start: AtomicU64,
    cycle_start: AtomicU64,
    cycle_end: AtomicU64,
    numerator: AtomicU32,
    denominator: AtomicU32,
    flags: AtomicU32,
}

impl Default for TimeState {
    fn default() -> Self {
        Self {
            sample_position: AtomicU64::new(0),
            ppq_position: AtomicU64::new(0),
            tempo: AtomicU64::new(0),
            bar_start: AtomicU64::new(0),
            cycle_start: AtomicU64::new(0),
            cycle_end: AtomicU64::new(0),
            numerator: AtomicU32::new(0),
            denominator: AtomicU32::new(0),
            flags: AtomicU32::new(0),
        }
    }
}

impl TimeState {
    fn update(&self, transport: Option<TransportInfo>) {
        let transport = transport.unwrap_or_default();
        let mut flags = 0;

        store_f64(
            &self.sample_position,
            transport.song_position_samples.unwrap_or(0) as f64,
        );

        if let Some(tempo) = transport.tempo_bpm {
            store_f64(&self.tempo, tempo);
            flags |= abi::KVST_TEMPO_VALID;
        } else {
            store_f64(&self.tempo, 0.0);
        }
        if let Some(beats) = transport.song_position_beats {
            store_f64(&self.ppq_position, beats);
            flags |= abi::KVST_PPQ_POS_VALID;
        } else {
            store_f64(&self.ppq_position, 0.0);
        }
        if let Some(bar) = transport.bar_start_beats {
            store_f64(&self.bar_start, bar);
            flags |= abi::KVST_BARS_VALID;
        } else {
            store_f64(&self.bar_start, 0.0);
        }
        if let Some((numerator, denominator)) = transport.time_signature {
            self.numerator.store(numerator, Ordering::Relaxed);
            self.denominator.store(denominator, Ordering::Relaxed);
            flags |= abi::KVST_TIME_SIG_VALID;
        } else {
            self.numerator.store(0, Ordering::Relaxed);
            self.denominator.store(0, Ordering::Relaxed);
        }

        store_f64(&self.cycle_start, 0.0);
        store_f64(&self.cycle_end, 0.0);

        if transport.playing {
            flags |= abi::KVST_TRANSPORT_PLAYING;
        }
        if transport.recording {
            flags |= abi::KVST_TRANSPORT_RECORDING;
        }
        if transport.loop_active {
            flags |= abi::KVST_TRANSPORT_CYCLE_ACTIVE;
        }
        #[allow(clippy::cast_possible_truncation)]
        self.flags.store(flags as u32, Ordering::Relaxed);
    }

    fn pointer(&self, sample_rate: f64) -> isize {
        let time = TimeInfo {
            sample_position: load_f64(&self.sample_position),
            sample_rate,
            nano_seconds: 0.0,
            ppq_position: load_f64(&self.ppq_position),
            tempo: load_f64(&self.tempo),
            bar_start_position: load_f64(&self.bar_start),
            cycle_start_position: load_f64(&self.cycle_start),
            cycle_end_position: load_f64(&self.cycle_end),
            time_sig_numerator: self.numerator.load(Ordering::Relaxed) as i32,
            time_sig_denominator: self.denominator.load(Ordering::Relaxed) as i32,
            smpte_offset: 0,
            smpte_frame_rate: 0,
            samples_to_next_clock: 0,
            flags: self.flags.load(Ordering::Relaxed) as i32,
        };
        TIME.with(|cell| {
            unsafe { cell.get().write(time) };
            cell.get() as isize
        })
    }
}

struct HostShared {
    sample_rate: AtomicU64,
    block_size: AtomicUsize,
    output_events: ArrayQueue<Event>,
    dropped_events: AtomicU32,
    editor_open: AtomicBool,
    editor_size: AtomicU64,
    io_changed: AtomicBool,
    time: TimeState,
    directory: Option<CString>,
}

impl HostShared {
    fn new(directory: Option<CString>) -> Self {
        Self {
            sample_rate: AtomicU64::new(DEFAULT_SAMPLE_RATE.to_bits()),
            block_size: AtomicUsize::new(DEFAULT_BLOCK_SIZE),
            output_events: ArrayQueue::new(MAX_EVENTS),
            dropped_events: AtomicU32::new(0),
            editor_open: AtomicBool::new(false),
            editor_size: AtomicU64::new(0),
            io_changed: AtomicBool::new(false),
            time: TimeState::default(),
            directory,
        }
    }

    fn take_editor_size(&self) -> Option<(u32, u32)> {
        let packed = self.editor_size.swap(0, Ordering::AcqRel);
        if packed == 0 {
            None
        } else {
            Some(((packed >> 32) as u32, packed as u32))
        }
    }
}

struct ProcessLevel(isize);

impl ProcessLevel {
    fn enter() -> Self {
        Self(PROCESS_LEVEL.replace(2))
    }
}

impl Drop for ProcessLevel {
    fn drop(&mut self) {
        PROCESS_LEVEL.set(self.0);
    }
}

fn store_f64(target: &AtomicU64, value: f64) {
    target.store(value.to_bits(), Ordering::Relaxed);
}

fn load_f64(source: &AtomicU64) -> f64 {
    f64::from_bits(source.load(Ordering::Relaxed))
}

fn dispatch(
    effect: *mut AEffect,
    opcode: i32,
    index: i32,
    value: isize,
    pointer: *mut c_void,
    option: f32,
) -> isize {
    if effect.is_null() {
        return 0;
    }
    let Some(dispatcher) = (unsafe { (*effect).dispatcher }) else {
        return 0;
    };
    unsafe { dispatcher(effect, opcode, index, value, pointer, option) }
}

fn dispatch_text(effect: *mut AEffect, opcode: i32, index: i32) -> String {
    let mut buffer = [0u8; 256];
    dispatch(effect, opcode, index, 0, buffer.as_mut_ptr().cast(), 0.0);
    let end = buffer
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(buffer.len());
    String::from_utf8_lossy(&buffer[..end]).into_owned()
}

fn copy_string(pointer: *mut c_void, text: &[u8]) -> isize {
    if pointer.is_null() {
        return 0;
    }
    let length = text.len().min(63);
    unsafe {
        ptr::copy_nonoverlapping(text.as_ptr(), pointer.cast::<u8>(), length);
        pointer.cast::<u8>().add(length).write(0);
    }
    1
}

fn can_do(pointer: *mut c_void) -> isize {
    if pointer.is_null() {
        return 0;
    }
    let mut bytes = [0u8; 128];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = unsafe { pointer.cast::<u8>().add(index).read() };
        if *byte == 0 {
            return isize::from(matches!(
                &bytes[..index],
                b"sendVstEvents"
                    | b"sendVstMidiEvent"
                    | b"receiveVstEvents"
                    | b"receiveVstMidiEvent"
                    | b"sendVstTimeInfo"
                    | b"sizeWindow"
                    | b"startStopProcess"
                    | b"shellCategory"
            ));
        }
    }
    0
}

unsafe extern "C" fn host_callback(
    effect: *mut AEffect,
    opcode: i32,
    index: i32,
    value: isize,
    pointer: *mut c_void,
    option: f32,
) -> isize {
    match opcode {
        abi::HOST_CURRENT_ID => return CURRENT_ID.with(Cell::get) as isize,
        abi::HOST_VERSION => return 2400,
        abi::HOST_GET_CURRENT_PROCESS_LEVEL => return PROCESS_LEVEL.with(Cell::get),
        abi::HOST_CAN_DO => return can_do(pointer),
        abi::HOST_GET_VENDOR_STRING | abi::HOST_GET_PRODUCT_STRING => {
            return copy_string(pointer, b"notwig-rack");
        }
        abi::HOST_GET_VENDOR_VERSION => return 11_005,
        abi::HOST_GET_LANGUAGE => return 1,
        _ => {}
    }

    if effect.is_null() {
        return match opcode {
            abi::HOST_GET_SAMPLE_RATE => DEFAULT_SAMPLE_RATE as isize,
            abi::HOST_GET_BLOCK_SIZE => DEFAULT_BLOCK_SIZE as isize,
            _ => 0,
        };
    }

    let state = unsafe { (*effect).resvd1.cast::<HostShared>() };
    if state.is_null() {
        return 0;
    }
    let state = unsafe { &*state };

    match opcode {
        abi::HOST_AUTOMATE => {
            if index >= 0 && option.is_finite() {
                push_event(
                    state,
                    Event {
                        sample_offset: 0,
                        body: EventBody::ParamValue {
                            param_id: index as u32,
                            value: f64::from(option),
                        },
                    },
                );
            }
            1
        }
        abi::HOST_GET_TIME => state.time.pointer(load_f64(&state.sample_rate)),
        abi::HOST_PROCESS_EVENTS => unsafe { receive_plugin_events(state, pointer) },
        abi::HOST_IO_CHANGED => {
            state.io_changed.store(true, Ordering::Release);
            1
        }
        abi::HOST_SIZE_WINDOW => {
            if state.editor_open.load(Ordering::Acquire)
                && (1..=8192).contains(&index)
                && (1..=8192).contains(&value)
            {
                state
                    .editor_size
                    .store(((index as u64) << 32) | value as u64, Ordering::Release);
                1
            } else {
                0
            }
        }
        abi::HOST_GET_SAMPLE_RATE => load_f64(&state.sample_rate) as isize,
        abi::HOST_GET_BLOCK_SIZE => state.block_size.load(Ordering::Relaxed) as isize,
        abi::HOST_GET_AUTOMATION_STATE => 1,
        abi::HOST_GET_DIRECTORY => state
            .directory
            .as_ref()
            .map_or(0, |directory| directory.as_ptr() as isize),
        abi::HOST_BEGIN_EDIT => {
            push_gesture(state, index, true);
            1
        }
        abi::HOST_END_EDIT => {
            push_gesture(state, index, false);
            1
        }
        _ => 0,
    }
}

fn push_event(state: &HostShared, event: Event) {
    if state.output_events.push(event).is_err() {
        state.dropped_events.fetch_add(1, Ordering::Relaxed);
    }
}

fn push_gesture(state: &HostShared, index: i32, active: bool) {
    if index < 0 {
        return;
    }
    push_event(
        state,
        Event {
            sample_offset: 0,
            body: EventBody::ParamGesture {
                param_id: index as u32,
                active,
            },
        },
    );
}

#[allow(clippy::cast_ptr_alignment)]
unsafe fn receive_plugin_events(state: &HostShared, pointer: *mut c_void) -> isize {
    if pointer.is_null() {
        return 0;
    }
    let base = pointer.cast::<u8>();
    let count = unsafe { base.cast::<i32>().read_unaligned() };
    if !(0..=MAX_EVENTS as i32).contains(&count) {
        state.dropped_events.fetch_add(1, Ordering::Relaxed);
        return 0;
    }

    let events = unsafe {
        base.add(std::mem::offset_of!(VstEventList, events))
            .cast::<*const c_void>()
    };
    for index in 0..count as usize {
        let event = unsafe { events.add(index).read_unaligned() };
        if event.is_null() {
            continue;
        }
        let kind = unsafe { event.cast::<i32>().read() };
        let byte_size = unsafe { event.cast::<i32>().add(1).read() };
        let readable = usize::try_from(byte_size).unwrap_or(0).saturating_add(8);
        match kind {
            abi::VST_MIDI_TYPE => {
                if readable < std::mem::size_of::<MidiEvent>() {
                    state.dropped_events.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
                let midi = unsafe { event.cast::<MidiEvent>().read() };
                let offset = u32::try_from(midi.delta_frames.max(0)).unwrap_or(0);
                let body = midi_body(midi.midi_data);
                push_event(
                    state,
                    Event {
                        sample_offset: offset,
                        body: EventBody::Midi(body),
                    },
                );
            }
            abi::VST_SYSEX_TYPE => {
                let needed =
                    std::mem::offset_of!(SysExEvent, sysex_dump) + std::mem::size_of::<*mut u8>();
                if readable < needed {
                    state.dropped_events.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
                let sysex = unsafe { event.cast::<SysExEvent>().read() };
                let length = usize::try_from(sysex.dump_bytes.max(0)).unwrap_or(0).min(8);
                if length == 0 || sysex.sysex_dump.is_null() {
                    continue;
                }
                let mut data = [0u8; 8];
                unsafe {
                    ptr::copy_nonoverlapping(sysex.sysex_dump, data.as_mut_ptr(), length);
                }
                let offset = u32::try_from(sysex.delta_frames.max(0)).unwrap_or(0);
                push_event(
                    state,
                    Event {
                        sample_offset: offset,
                        body: EventBody::Midi(MidiData::Raw {
                            len: length as u8,
                            data,
                        }),
                    },
                );
            }
            _ => {
                state.dropped_events.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    1
}

struct RawMidi {
    len: u8,
    data: [u8; 8],
}

fn midi_to_bytes(midi: MidiData) -> Option<RawMidi> {
    let (length, data): (u8, [u8; 8]) = match midi {
        MidiData::NoteOn {
            channel,
            note,
            velocity,
        } => (3, [0x90 | (channel & 0x0F), note, velocity, 0, 0, 0, 0, 0]),
        MidiData::NoteOff {
            channel,
            note,
            velocity,
        } => (3, [0x80 | (channel & 0x0F), note, velocity, 0, 0, 0, 0, 0]),
        MidiData::PolyAftertouch {
            channel,
            note,
            pressure,
        } => (3, [0xA0 | (channel & 0x0F), note, pressure, 0, 0, 0, 0, 0]),
        MidiData::ControlChange {
            channel,
            controller,
            value,
        } => (
            3,
            [0xB0 | (channel & 0x0F), controller, value, 0, 0, 0, 0, 0],
        ),
        MidiData::ProgramChange { channel, program } => {
            (2, [0xC0 | (channel & 0x0F), program, 0, 0, 0, 0, 0, 0])
        }
        MidiData::ChannelAftertouch { channel, pressure } => {
            (2, [0xD0 | (channel & 0x0F), pressure, 0, 0, 0, 0, 0, 0])
        }
        MidiData::PitchBend { channel, value } => (
            3,
            [
                0xE0 | (channel & 0x0F),
                (value & 0x7F) as u8,
                ((value >> 7) & 0x7F) as u8,
                0,
                0,
                0,
                0,
                0,
            ],
        ),
        MidiData::Raw { len, data } => (len.min(8), data),
    };
    if length == 0 {
        return None;
    }
    Some(RawMidi { len: length, data })
}

fn midi_body(bytes: [u8; 4]) -> MidiData {
    let status = bytes[0];
    let channel = status & 0x0F;
    match status & 0xF0 {
        0x80 => MidiData::NoteOff {
            channel,
            note: bytes[1],
            velocity: bytes[2],
        },
        0x90 => {
            if bytes[2] == 0 {
                MidiData::NoteOff {
                    channel,
                    note: bytes[1],
                    velocity: 0,
                }
            } else {
                MidiData::NoteOn {
                    channel,
                    note: bytes[1],
                    velocity: bytes[2],
                }
            }
        }
        0xA0 => MidiData::PolyAftertouch {
            channel,
            note: bytes[1],
            pressure: bytes[2],
        },
        0xB0 => MidiData::ControlChange {
            channel,
            controller: bytes[1],
            value: bytes[2],
        },
        0xC0 => MidiData::ProgramChange {
            channel,
            program: bytes[1],
        },
        0xD0 => MidiData::ChannelAftertouch {
            channel,
            pressure: bytes[1],
        },
        0xE0 => MidiData::PitchBend {
            channel,
            value: u16::from(bytes[1]) | (u16::from(bytes[2]) << 7),
        },
        _ => MidiData::Raw {
            len: 3,
            data: [bytes[0], bytes[1], bytes[2], 0, 0, 0, 0, 0],
        },
    }
}

fn push_midi_event(
    list: &mut VstEventList,
    events: &mut [MidiEvent; MAX_EVENTS],
    offset: u32,
    bytes: &[u8; 8],
) {
    let index = list.num_events as usize;
    if index >= MAX_EVENTS {
        return;
    }
    let mut data = [0u8; 4];
    data.copy_from_slice(&bytes[..4]);
    events[index] = MidiEvent {
        kind: abi::VST_MIDI_TYPE,
        byte_size: (std::mem::size_of::<MidiEvent>() - 8) as i32,
        delta_frames: i32::try_from(offset).unwrap_or(i32::MAX),
        flags: 0,
        note_length: 0,
        note_offset: 0,
        midi_data: data,
        detune: 0,
        note_off_velocity: 0,
        reserved1: 0,
        reserved2: 0,
    };
    list.events[index] = (&raw const events[index]).cast();
    list.num_events += 1;
}

fn push_sysex_event(
    list: &mut VstEventList,
    events: &mut [SysExEvent; MAX_EVENTS],
    bytes: &mut [[u8; 8]; MAX_EVENTS],
    offset: u32,
    data: &[u8],
) {
    let index = list.num_events as usize;
    if index >= MAX_EVENTS {
        return;
    }
    let length = data.len().min(8);
    bytes[index][..length].copy_from_slice(&data[..length]);
    events[index] = SysExEvent {
        kind: abi::VST_SYSEX_TYPE,
        byte_size: (std::mem::size_of::<SysExEvent>() - 8) as i32,
        delta_frames: i32::try_from(offset).unwrap_or(i32::MAX),
        flags: 0,
        dump_bytes: length as i32,
        reserved1: 0,
        sysex_dump: bytes[index].as_mut_ptr(),
        reserved2: 0,
    };
    list.events[index] = (&raw const events[index]).cast();
    list.num_events += 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_from_channel_counts() {
        let layout = layout_for(2, 2);
        assert_eq!(layout.total_input_channels(), 2);
        assert_eq!(layout.total_output_channels(), 2);
        let mono = layout_for(1, 1);
        assert_eq!(mono.total_input_channels(), 1);
        assert_eq!(mono.total_output_channels(), 1);
        let instrument = layout_for(0, 2);
        assert_eq!(instrument.total_input_channels(), 0);
        assert_eq!(instrument.total_output_channels(), 2);
    }

    #[test]
    fn midi_body_decodes_channel_voice() {
        let note = midi_body([0x90, 60, 100, 0]);
        assert!(matches!(
            note,
            MidiData::NoteOn {
                channel: 0,
                note: 60,
                velocity: 100
            }
        ));
        let off = midi_body([0x90, 60, 0, 0]);
        assert!(matches!(
            off,
            MidiData::NoteOff {
                channel: 0,
                note: 60,
                velocity: 0
            }
        ));
        let bend = midi_body([0xE1, 0x00, 0x40, 0]);
        assert!(matches!(
            bend,
            MidiData::PitchBend {
                channel: 1,
                value: 8192
            }
        ));
    }

    #[test]
    fn midi_to_bytes_round_trips_note() {
        let raw = midi_to_bytes(MidiData::NoteOn {
            channel: 3,
            note: 64,
            velocity: 127,
        })
        .expect("bytes");
        assert_eq!(raw.len, 3);
        assert_eq!(&raw.data[..3], &[0x93, 64, 127]);
    }

    #[test]
    fn host_callback_receives_midi_events() {
        let shared = HostShared::new(None);
        let mut effect = AEffect {
            resvd1: (&raw const shared).cast_mut().cast(),
            ..AEffect::default()
        };

        let midi = MidiEvent {
            kind: abi::VST_MIDI_TYPE,
            byte_size: (std::mem::size_of::<MidiEvent>() - 8) as i32,
            delta_frames: 3,
            midi_data: [0x90, 60, 100, 0],
            ..MidiEvent::default()
        };
        let mut list = VstEventList {
            num_events: 1,
            events: [(&raw const midi).cast(); MAX_EVENTS],
            ..VstEventList::default()
        };
        let result = unsafe {
            host_callback(
                &raw mut effect,
                abi::HOST_PROCESS_EVENTS,
                0,
                0,
                (&raw mut list).cast(),
                0.0,
            )
        };
        assert_eq!(result, 1);
        let event = shared.output_events.pop().expect("event");
        assert_eq!(event.sample_offset, 3);
        assert!(matches!(
            event.body,
            EventBody::Midi(MidiData::NoteOn {
                channel: 0,
                note: 60,
                velocity: 100
            })
        ));
    }

    #[test]
    fn host_callback_records_editor_resize() {
        let shared = HostShared::new(None);
        shared.editor_open.store(true, Ordering::Release);
        let mut effect = AEffect {
            resvd1: (&raw const shared).cast_mut().cast(),
            ..AEffect::default()
        };
        let result = unsafe {
            host_callback(
                &raw mut effect,
                abi::HOST_SIZE_WINDOW,
                640,
                480,
                ptr::null_mut(),
                0.0,
            )
        };
        assert_eq!(result, 1);
        assert_eq!(shared.take_editor_size(), Some((640, 480)));
    }

    #[test]
    fn time_state_reports_transport() {
        let state = TimeState::default();
        state.update(Some(TransportInfo {
            tempo_bpm: Some(128.0),
            time_signature: Some((7, 8)),
            song_position_beats: Some(16.0),
            song_position_samples: Some(48_000),
            bar_start_beats: Some(12.0),
            playing: true,
            recording: false,
            loop_active: true,
        }));
        let pointer = state.pointer(48_000.0) as *const TimeInfo;
        let time = unsafe { *pointer };
        assert!((time.tempo - 128.0).abs() < f64::EPSILON);
        assert!((time.ppq_position - 16.0).abs() < f64::EPSILON);
        assert_eq!(time.time_sig_numerator, 7);
        assert_eq!(time.time_sig_denominator, 8);
        assert_ne!(time.flags & abi::KVST_TRANSPORT_PLAYING, 0);
        assert_ne!(time.flags & abi::KVST_TRANSPORT_CYCLE_ACTIVE, 0);
    }

    #[test]
    fn zero_editor_rect_is_rejected() {
        assert_eq!(
            rect_size(Rect {
                top: 0,
                left: 0,
                bottom: 0,
                right: 0,
            }),
            None
        );
        assert_eq!(
            rect_size(Rect {
                top: 0,
                left: 0,
                bottom: 480,
                right: 640,
            }),
            Some((640, 480))
        );
    }

    #[test]
    fn default_paths_contains_a_vst_directory() {
        let paths = default_vst2_paths();
        assert!(paths.iter().any(|path| {
            path.file_name()
                .and_then(OsStr::to_str)
                .is_some_and(|name| {
                    name.eq_ignore_ascii_case("vst")
                        || name.eq_ignore_ascii_case("vstplugins")
                        || name.eq_ignore_ascii_case("vst2")
                })
        }));
    }
}
