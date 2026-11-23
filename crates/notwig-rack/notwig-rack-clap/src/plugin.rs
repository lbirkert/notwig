//! Plugin instance core: loading, parameters, state, activation, and teardown.

use crate::editor::gui_supports_current_platform;
use crate::events::{ConvertedInputEvents, OutputEventsSink};
use crate::host::HostState;
use crate::library::{LoadedLibrary, bundle_binary_path};

use notwig_rack_core::bus::BusLayout;
use notwig_rack_core::error::{Error, Result};
use notwig_rack_core::info::{ParameterInfo, PluginInfo, PresetInfo};
use notwig_rack_core::plugin::PluginCore;

use clap_sys::ext::gui::{CLAP_EXT_GUI, clap_plugin_gui};
use clap_sys::ext::params::{
    CLAP_EXT_PARAMS, CLAP_PARAM_IS_AUTOMATABLE, CLAP_PARAM_IS_BYPASS, CLAP_PARAM_IS_ENUM,
    CLAP_PARAM_IS_HIDDEN, CLAP_PARAM_IS_READONLY, CLAP_PARAM_IS_STEPPED, clap_param_info,
    clap_plugin_params,
};
use clap_sys::ext::state::{CLAP_EXT_STATE, clap_plugin_state};
use clap_sys::plugin::clap_plugin;
use clap_sys::stream::{clap_istream, clap_ostream};

use std::ffi::{CStr, CString, c_char};
use std::path::PathBuf;
use std::ptr;

/// One loaded CLAP plugin instance.
///
/// Holds the raw `*const clap_plugin` pointer plus the metadata
/// needed for the notwig-rack-core trait impls. The pointer is owned —
/// `Drop` calls `clap_plugin::destroy` and `clap_entry::deinit`.
pub struct ClapPlugin {
    info: PluginInfo,
    layouts: Vec<BusLayout>,
    active_layout: Option<BusLayout>,
    pub(crate) plugin: *const clap_plugin,
    _host: Box<HostState>,
    library: LoadedLibrary,
    bundle_path: PathBuf,
    pub(crate) started_processing: bool,
    /// Cached `clap.params` extension. NULL if the plugin doesn't
    /// expose any parameters.
    params_ext: *const clap_plugin_params,
    /// Cached `clap.state` extension. NULL if the plugin doesn't
    /// expose state save/load.
    state_ext: *const clap_plugin_state,
    /// Cached `clap.gui` extension. NULL when the plugin has no
    /// custom editor.
    pub(crate) gui_ext: *const clap_plugin_gui,
    /// Whether the editor has been `create`d but not yet
    /// `destroy`ed — `clap.gui` separates the two lifecycles.
    pub(crate) gui_open: bool,
    /// Running `steady_time` counter — CLAP wants a monotonically
    /// increasing sample-count across activations.
    pub(crate) steady_time: i64,
    /// Parameter changes queued from `set_parameter` while the
    /// plugin is processing; drained into the next `process` call's
    /// input events.
    pub(crate) pending_param_changes: Vec<(u32, f64)>,
    /// Reused CLAP event storage so per-block MIDI translation does not
    /// allocate once the event count reaches steady state.
    pub(crate) input_events: ConvertedInputEvents,
    /// Reused channel-pointer slabs for the CLAP audio buffers. They are only
    /// valid for the duration of one `process` call.
    pub(crate) input_ptrs: Vec<*mut f32>,
    pub(crate) output_ptrs: Vec<*mut f32>,
}

// SAFETY: The CLAP spec is explicit that a plugin handle may be
// moved between threads when audio processing is stopped. Within
// notwig-rack-clap we serialize all calls through `&mut self`, so the
// pointer is never aliased.
unsafe impl Send for ClapPlugin {}

impl ClapPlugin {
    pub(crate) fn load_from(info: &PluginInfo) -> Result<Self> {
        let bundle_path = info.path.clone();
        let binary = bundle_binary_path(&bundle_path);
        let library = unsafe { LoadedLibrary::open(&binary)? };
        let entry = library.entry()?;
        unsafe { entry.init(&binary)? };
        let factory = unsafe { entry.factory() };
        if factory.is_null() {
            unsafe { entry.deinit() };
            return Err(Error::LoadFailed {
                path: bundle_path,
                reason: "plugin has no clap.plugin-factory".into(),
            });
        }
        let create = unsafe { (*factory).create_plugin }.ok_or_else(|| Error::LoadFailed {
            path: bundle_path.clone(),
            reason: "factory missing create_plugin".into(),
        })?;
        let id_cstring = CString::new(info.unique_id.as_str()).map_err(|e| Error::LoadFailed {
            path: bundle_path.clone(),
            reason: format!("plugin id is not a valid C string: {e}"),
        })?;
        let host = HostState::new();
        let plugin = unsafe { create(factory, &raw const host.host, id_cstring.as_ptr()) };
        if plugin.is_null() {
            unsafe { entry.deinit() };
            return Err(Error::LoadFailed {
                path: bundle_path,
                reason: "factory.create_plugin returned NULL".into(),
            });
        }
        let init = unsafe { (*plugin).init }.ok_or_else(|| Error::LoadFailed {
            path: bundle_path.clone(),
            reason: "plugin missing init".into(),
        })?;
        if !unsafe { init(plugin) } {
            if let Some(destroy) = unsafe { (*plugin).destroy } {
                unsafe { destroy(plugin) };
            }
            unsafe { entry.deinit() };
            return Err(Error::LoadFailed {
                path: bundle_path,
                reason: "clap_plugin::init returned false".into(),
            });
        }
        let params_ext = unsafe { lookup_extension::<clap_plugin_params>(plugin, CLAP_EXT_PARAMS) };
        let state_ext = unsafe { lookup_extension::<clap_plugin_state>(plugin, CLAP_EXT_STATE) };
        let gui_ext = unsafe { lookup_extension::<clap_plugin_gui>(plugin, CLAP_EXT_GUI) };
        let mut info = info.clone();
        if !gui_ext.is_null() && unsafe { gui_supports_current_platform(plugin, gui_ext) } {
            info.has_editor = true;
        }
        Ok(Self {
            info,
            layouts: vec![BusLayout::stereo()],
            active_layout: None,
            plugin,
            _host: host,
            library,
            bundle_path,
            started_processing: false,
            params_ext,
            state_ext,
            gui_ext,
            gui_open: false,
            steady_time: 0,
            pending_param_changes: Vec::new(),
            input_events: ConvertedInputEvents::default(),
            input_ptrs: Vec::new(),
            output_ptrs: Vec::new(),
        })
    }
}

/// Look up a CLAP extension by id. Returns `null` if the plugin
/// doesn't implement that extension. The returned pointer's
/// lifetime is the plugin instance; callers must not outlive
/// the plugin.
unsafe fn lookup_extension<T>(plugin: *const clap_plugin, id: &CStr) -> *const T {
    let Some(get_extension) = (unsafe { (*plugin).get_extension }) else {
        return ptr::null();
    };
    let raw = unsafe { get_extension(plugin, id.as_ptr()) };
    raw.cast::<T>()
}

impl Drop for ClapPlugin {
    fn drop(&mut self) {
        if !self.plugin.is_null() {
            if self.gui_open
                && let Some(destroy) = unsafe { (*self.gui_ext).destroy }
            {
                unsafe { destroy(self.plugin) };
                self.gui_open = false;
            }
            if self.started_processing
                && let Some(stop) = unsafe { (*self.plugin).stop_processing }
            {
                unsafe { stop(self.plugin) };
            }
            if self.active_layout.is_some()
                && let Some(deactivate) = unsafe { (*self.plugin).deactivate }
            {
                unsafe { deactivate(self.plugin) };
            }
            if let Some(destroy) = unsafe { (*self.plugin).destroy } {
                unsafe { destroy(self.plugin) };
            }
        }
        if let Ok(entry) = self.library.entry() {
            unsafe { entry.deinit() };
        }
        // `library` drops here, unloading the dylib.
        let _ = &self.bundle_path; // keep PathBuf alive for diagnostics
    }
}

impl PluginCore for ClapPlugin {
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
        if self.params_ext.is_null() {
            return 0;
        }
        let count = unsafe { (*self.params_ext).count };
        count.map_or(0, |c| unsafe { c(self.plugin) as usize })
    }

    fn parameter_info(&self, index: usize) -> Result<ParameterInfo> {
        if self.params_ext.is_null() {
            return Err(Error::InvalidParameter(index));
        }
        let get_info =
            unsafe { (*self.params_ext).get_info }.ok_or(Error::InvalidParameter(index))?;
        let mut info = empty_param_info();
        let idx_u32 = u32::try_from(index).map_err(|_| Error::InvalidParameter(index))?;
        let ok = unsafe { get_info(self.plugin, idx_u32, &raw mut info) };
        if !ok {
            return Err(Error::InvalidParameter(index));
        }
        Ok(clap_param_info_to_rack(&info))
    }

    fn parameter_value(&self, index: usize) -> Result<f64> {
        if self.params_ext.is_null() {
            return Err(Error::InvalidParameter(index));
        }
        // Resolve index -> id via get_info, then ask get_value
        // for the current value. CLAP's params API is id-keyed,
        // not index-keyed.
        let info = self.parameter_info(index)?;
        let get_value =
            unsafe { (*self.params_ext).get_value }.ok_or(Error::InvalidParameter(index))?;
        let mut out = 0.0f64;
        let ok = unsafe { get_value(self.plugin, info.id, &raw mut out) };
        if !ok {
            return Err(Error::InvalidParameter(index));
        }
        Ok(out)
    }

    fn parameter_value_string(&self, index: usize, value: f64) -> Result<String> {
        if self.params_ext.is_null() {
            return Err(Error::InvalidParameter(index));
        }
        let info = self.parameter_info(index)?;
        let value_to_text =
            unsafe { (*self.params_ext).value_to_text }.ok_or(Error::InvalidParameter(index))?;
        let mut buf = [0i8; clap_sys::string_sizes::CLAP_NAME_SIZE];
        let buf_len = u32::try_from(buf.len()).unwrap_or(u32::MAX);
        let ok = unsafe { value_to_text(self.plugin, info.id, value, buf.as_mut_ptr(), buf_len) };
        if !ok {
            return Err(Error::InvalidParameter(index));
        }
        Ok(c_buf_to_string(&buf))
    }

    fn set_parameter(&mut self, index: usize, value: f64) -> Result<()> {
        if self.params_ext.is_null() {
            return Err(Error::InvalidParameter(index));
        }
        let info = self.parameter_info(index)?;
        // Queue the change either way; process() drains the queue
        // every block, and a non-processing host can call flush()
        // to apply it immediately.
        self.pending_param_changes.push((info.id, value));
        if !self.started_processing
            && let Some(flush) = unsafe { (*self.params_ext).flush }
        {
            let events = std::mem::take(&mut self.pending_param_changes);
            let mut converted = ConvertedInputEvents { events: Vec::new() };
            for (param_id, value) in events {
                converted.push_param_value(0, param_id, value);
            }
            let input = converted.as_clap();
            let mut sink = OutputEventsSink::new(None);
            let output = sink.as_clap();
            unsafe { flush(self.plugin, &raw const input, &raw const output) };
        }
        Ok(())
    }

    fn preset_count(&self) -> usize {
        0
    }

    fn preset_info(&self, index: usize) -> Result<PresetInfo> {
        Err(Error::InvalidParameter(index))
    }

    fn load_preset(&mut self, _preset_number: i32) -> Result<()> {
        Err(Error::Other("clap preset loading not yet wired".into()))
    }

    fn save_state(&self) -> Result<Vec<u8>> {
        if self.state_ext.is_null() {
            return Err(Error::Other("plugin missing clap.state extension".into()));
        }
        let save = unsafe { (*self.state_ext).save }
            .ok_or_else(|| Error::Other("clap.state extension missing save fn".into()))?;
        let mut buffer = WriteBuffer::default();
        let stream = clap_ostream {
            ctx: (&raw mut buffer).cast(),
            write: Some(ostream_write),
        };
        let ok = unsafe { save(self.plugin, &raw const stream) };
        if !ok {
            return Err(Error::Other("clap state save returned false".into()));
        }
        Ok(buffer.bytes)
    }

    fn load_state(&mut self, bytes: &[u8]) -> Result<()> {
        if self.state_ext.is_null() {
            return Err(Error::Other("plugin missing clap.state extension".into()));
        }
        let load = unsafe { (*self.state_ext).load }
            .ok_or_else(|| Error::Other("clap.state extension missing load fn".into()))?;
        let mut cursor = ReadCursor { bytes, position: 0 };
        let stream = clap_istream {
            ctx: (&raw mut cursor).cast(),
            read: Some(istream_read),
        };
        let ok = unsafe { load(self.plugin, &raw const stream) };
        if !ok {
            return Err(Error::Other("clap state load returned false".into()));
        }
        Ok(())
    }

    fn activate(
        &mut self,
        layout: BusLayout,
        sample_rate: f64,
        max_block_size: usize,
    ) -> Result<()> {
        let Some(activate) = (unsafe { (*self.plugin).activate }) else {
            return Err(Error::Other("clap plugin missing activate".into()));
        };
        let ok = unsafe {
            activate(
                self.plugin,
                sample_rate,
                1,
                u32::try_from(max_block_size).unwrap_or(u32::MAX),
            )
        };
        if !ok {
            return Err(Error::Other("clap_plugin::activate returned false".into()));
        }
        self.active_layout = Some(layout);
        Ok(())
    }

    fn deactivate(&mut self) {
        if self.started_processing {
            if let Some(stop) = unsafe { (*self.plugin).stop_processing } {
                unsafe { stop(self.plugin) };
            }
            self.started_processing = false;
        }
        if let Some(deactivate) = unsafe { (*self.plugin).deactivate } {
            unsafe { deactivate(self.plugin) };
        }
        self.active_layout = None;
    }

    fn is_active(&self) -> bool {
        self.active_layout.is_some()
    }

    fn editor(&mut self) -> Option<&mut dyn notwig_rack_core::editor::PluginEditor> {
        if self.gui_ext.is_null() {
            return None;
        }
        Some(self)
    }
}

fn empty_param_info() -> clap_param_info {
    clap_param_info {
        id: 0,
        flags: 0,
        cookie: ptr::null_mut(),
        name: [0; clap_sys::string_sizes::CLAP_NAME_SIZE],
        module: [0; clap_sys::string_sizes::CLAP_PATH_SIZE],
        min_value: 0.0,
        max_value: 0.0,
        default_value: 0.0,
    }
}

fn clap_param_info_to_rack(info: &clap_param_info) -> ParameterInfo {
    let name = c_buf_to_string(&info.name);
    let mut flags = notwig_rack_core::info::ParameterFlags::empty();
    if info.flags & CLAP_PARAM_IS_BYPASS != 0 {
        flags |= notwig_rack_core::info::ParameterFlags::BYPASS;
    }
    if info.flags & CLAP_PARAM_IS_AUTOMATABLE != 0 {
        flags |= notwig_rack_core::info::ParameterFlags::AUTOMATABLE;
    }
    if info.flags & CLAP_PARAM_IS_HIDDEN != 0 {
        flags |= notwig_rack_core::info::ParameterFlags::HIDDEN;
    }
    if info.flags & CLAP_PARAM_IS_READONLY != 0 {
        flags |= notwig_rack_core::info::ParameterFlags::READ_ONLY;
    }
    if info.flags & CLAP_PARAM_IS_ENUM != 0 {
        flags |= notwig_rack_core::info::ParameterFlags::ENUMERATED;
    }
    let step_count = if info.flags & CLAP_PARAM_IS_STEPPED != 0 {
        // Stepped parameters have integer-valued [min, max]; the
        // step count is the number of distinct values.
        #[allow(clippy::cast_possible_truncation)]
        let span = (info.max_value - info.min_value).round() as i64;
        u32::try_from(span + 1).unwrap_or(0)
    } else {
        0
    };
    ParameterInfo {
        id: info.id,
        name: name.clone(),
        short_name: name,
        unit: String::new(), // CLAP doesn't expose a separate unit field
        min: info.min_value,
        max: info.max_value,
        default: info.default_value,
        step_count,
        flags,
    }
}

#[allow(clippy::cast_sign_loss)]
fn c_buf_to_string(buf: &[c_char]) -> String {
    // CLAP `char` is signed on Apple toolchains, unsigned elsewhere;
    // the cast preserves bit pattern, which is what
    // `String::from_utf8_lossy` wants.
    let bytes: Vec<u8> = buf
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| b as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Sink for `clap_plugin_state::save` — the plugin pushes bytes
/// via the C `write` callback, we accumulate into a Vec.
#[derive(Default)]
struct WriteBuffer {
    bytes: Vec<u8>,
}

unsafe extern "C" fn ostream_write(
    stream: *const clap_ostream,
    buffer: *const std::ffi::c_void,
    size: u64,
) -> i64 {
    if stream.is_null() || buffer.is_null() {
        return -1;
    }
    let ctx = unsafe { (*stream).ctx.cast::<WriteBuffer>() };
    if ctx.is_null() {
        return -1;
    }
    let Ok(size_usize) = usize::try_from(size) else {
        return -1;
    };
    let slice = unsafe { std::slice::from_raw_parts(buffer.cast::<u8>(), size_usize) };
    unsafe { (*ctx).bytes.extend_from_slice(slice) };
    i64::try_from(size_usize).unwrap_or(i64::MAX)
}

/// Source for `clap_plugin_state::load` — the plugin pulls bytes
/// via the C `read` callback, we hand back from a `&[u8]`.
struct ReadCursor<'a> {
    bytes: &'a [u8],
    position: usize,
}

unsafe extern "C" fn istream_read(
    stream: *const clap_istream,
    buffer: *mut std::ffi::c_void,
    size: u64,
) -> i64 {
    if stream.is_null() || buffer.is_null() {
        return -1;
    }
    let ctx = unsafe { (*stream).ctx.cast::<ReadCursor<'_>>() };
    if ctx.is_null() {
        return -1;
    }
    let cursor = unsafe { &mut *ctx };
    let Ok(want) = usize::try_from(size) else {
        return -1;
    };
    let available = cursor.bytes.len().saturating_sub(cursor.position);
    let take = want.min(available);
    if take > 0 {
        unsafe {
            std::ptr::copy_nonoverlapping(
                cursor.bytes.as_ptr().add(cursor.position),
                buffer.cast::<u8>(),
                take,
            );
        }
        cursor.position += take;
    }
    i64::try_from(take).unwrap_or(i64::MAX)
}
