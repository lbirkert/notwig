//! The `clap_host` object and its extension table.
//!
//! CLAP requires a non-null host and some plugins (notably nih-plug) assert on
//! it during construction. The host stays alive for the plugin instance and
//! advertises the safe, no-op extension surface the host currently supports.

use clap_sys::ext::audio_ports::{CLAP_EXT_AUDIO_PORTS, clap_host_audio_ports};
use clap_sys::ext::gui::{CLAP_EXT_GUI, clap_host_gui};
use clap_sys::ext::latency::{CLAP_EXT_LATENCY, clap_host_latency};
use clap_sys::ext::note_ports::{
    CLAP_EXT_NOTE_PORTS, CLAP_NOTE_DIALECT_CLAP, clap_host_note_ports,
};
use clap_sys::ext::params::{CLAP_EXT_PARAMS, clap_host_params};
use clap_sys::ext::remote_controls::{CLAP_EXT_REMOTE_CONTROLS, clap_host_remote_controls};
use clap_sys::ext::state::{CLAP_EXT_STATE, clap_host_state};
use clap_sys::ext::tail::{CLAP_EXT_TAIL, clap_host_tail};
use clap_sys::ext::voice_info::{CLAP_EXT_VOICE_INFO, clap_host_voice_info};
use clap_sys::host::clap_host;
use clap_sys::version::CLAP_VERSION;

use std::ffi::{CStr, CString, c_char, c_void};
use std::ptr;

/// Minimal CLAP host object passed to `clap_plugin_factory::create_plugin`.
///
/// CLAP requires a non-null host and some plugins (notably nih-plug) assert on
/// it during construction. The host stays alive for the plugin instance and
/// advertises the safe, no-op extension surface the host currently supports.
#[allow(dead_code)]
pub(crate) struct HostState {
    pub(crate) host: clap_host,
    params: clap_host_params,
    latency: clap_host_latency,
    gui: clap_host_gui,
    voice_info: clap_host_voice_info,
    audio_ports: clap_host_audio_ports,
    note_ports: clap_host_note_ports,
    state_ext: clap_host_state,
    tail: clap_host_tail,
    remote_controls: clap_host_remote_controls,
    name: CString,
    vendor: CString,
    url: CString,
    version: CString,
}

impl HostState {
    pub(crate) fn new() -> Box<Self> {
        let name = CString::new("Notwig").unwrap_or_default();
        let vendor = CString::new("Notwig").unwrap_or_default();
        let url = CString::new("https://notwig.lbirkert.com").unwrap_or_default();
        let version = CString::new(env!("CARGO_PKG_VERSION")).unwrap_or_default();
        let mut state = Box::new(Self {
            host: clap_host {
                clap_version: CLAP_VERSION,
                host_data: ptr::null_mut(),
                name: name.as_ptr(),
                vendor: vendor.as_ptr(),
                url: url.as_ptr(),
                version: version.as_ptr(),
                get_extension: Some(host_get_extension),
                request_restart: Some(host_request_restart),
                request_process: Some(host_request_process),
                request_callback: Some(host_request_callback),
            },
            params: clap_host_params {
                rescan: Some(params_rescan),
                clear: Some(params_clear),
                request_flush: Some(params_request_flush),
            },
            latency: clap_host_latency {
                changed: Some(latency_changed),
            },
            gui: clap_host_gui {
                resize_hints_changed: Some(gui_resize_hints_changed),
                request_resize: Some(gui_request_resize),
                request_show: Some(gui_request_show),
                request_hide: Some(gui_request_hide),
                closed: Some(gui_closed),
            },
            voice_info: clap_host_voice_info {
                changed: Some(voice_info_changed),
            },
            audio_ports: clap_host_audio_ports {
                is_rescan_flag_supported: Some(audio_ports_is_rescan_flag_supported),
                rescan: Some(audio_ports_rescan),
            },
            note_ports: clap_host_note_ports {
                supported_dialects: Some(note_ports_supported_dialects),
                rescan: Some(note_ports_rescan),
            },
            state_ext: clap_host_state {
                mark_dirty: Some(state_mark_dirty),
            },
            tail: clap_host_tail {
                changed: Some(tail_changed),
            },
            remote_controls: clap_host_remote_controls {
                changed: Some(remote_controls_changed),
                suggest_page: Some(remote_controls_suggest_page),
            },
            name,
            vendor,
            url,
            version,
        });
        state.host.host_data = (&raw mut *state).cast();
        state
    }
}

unsafe fn host_state<'a>(host: *const clap_host) -> Option<&'a HostState> {
    if host.is_null() {
        return None;
    }
    let data = unsafe { (*host).host_data as *const HostState };
    if data.is_null() {
        None
    } else {
        Some(unsafe { &*data })
    }
}

unsafe extern "C" fn host_get_extension(
    host: *const clap_host,
    id: *const c_char,
) -> *const c_void {
    let Some(state) = (unsafe { host_state(host) }) else {
        return ptr::null();
    };
    if id.is_null() {
        return ptr::null();
    }
    let id = unsafe { CStr::from_ptr(id) };
    if id == CLAP_EXT_PARAMS {
        (&raw const state.params).cast()
    } else if id == CLAP_EXT_LATENCY {
        (&raw const state.latency).cast()
    } else if id == CLAP_EXT_GUI {
        (&raw const state.gui).cast()
    } else if id == CLAP_EXT_VOICE_INFO {
        (&raw const state.voice_info).cast()
    } else if id == CLAP_EXT_AUDIO_PORTS {
        (&raw const state.audio_ports).cast()
    } else if id == CLAP_EXT_NOTE_PORTS {
        (&raw const state.note_ports).cast()
    } else if id == CLAP_EXT_STATE {
        (&raw const state.state_ext).cast()
    } else if id == CLAP_EXT_TAIL {
        (&raw const state.tail).cast()
    } else if id == CLAP_EXT_REMOTE_CONTROLS {
        (&raw const state.remote_controls).cast()
    } else {
        ptr::null()
    }
}

unsafe extern "C" fn host_request_restart(_host: *const clap_host) {}
unsafe extern "C" fn host_request_process(_host: *const clap_host) {}
unsafe extern "C" fn host_request_callback(_host: *const clap_host) {}
unsafe extern "C" fn params_rescan(_host: *const clap_host, _flags: u32) {}
unsafe extern "C" fn params_clear(_host: *const clap_host, _param_id: u32, _flags: u32) {}
unsafe extern "C" fn params_request_flush(_host: *const clap_host) {}
unsafe extern "C" fn latency_changed(_host: *const clap_host) {}
unsafe extern "C" fn gui_resize_hints_changed(_host: *const clap_host) {}
unsafe extern "C" fn gui_request_resize(
    _host: *const clap_host,
    _width: u32,
    _height: u32,
) -> bool {
    false
}
unsafe extern "C" fn gui_request_show(_host: *const clap_host) -> bool {
    false
}
unsafe extern "C" fn gui_request_hide(_host: *const clap_host) -> bool {
    false
}
unsafe extern "C" fn gui_closed(_host: *const clap_host, _was_destroyed: bool) {}
unsafe extern "C" fn voice_info_changed(_host: *const clap_host) {}
unsafe extern "C" fn audio_ports_is_rescan_flag_supported(
    _host: *const clap_host,
    _flag: u32,
) -> bool {
    false
}
unsafe extern "C" fn audio_ports_rescan(_host: *const clap_host, _flags: u32) {}
unsafe extern "C" fn note_ports_supported_dialects(_host: *const clap_host) -> u32 {
    CLAP_NOTE_DIALECT_CLAP
}
unsafe extern "C" fn note_ports_rescan(_host: *const clap_host, _flags: u32) {}
unsafe extern "C" fn state_mark_dirty(_host: *const clap_host) {}
unsafe extern "C" fn tail_changed(_host: *const clap_host) {}
unsafe extern "C" fn remote_controls_changed(_host: *const clap_host) {}
unsafe extern "C" fn remote_controls_suggest_page(_host: *const clap_host, _page_id: u32) {}

#[cfg(test)]
mod host_tests {
    use super::*;

    #[test]
    fn host_object_exposes_core_extensions() {
        let host = HostState::new();
        let get = host.host.get_extension.expect("host callback");
        unsafe {
            assert!(!get(&raw const host.host, CLAP_EXT_PARAMS.as_ptr()).is_null());
            assert!(!get(&raw const host.host, CLAP_EXT_LATENCY.as_ptr()).is_null());
            assert!(!get(&raw const host.host, CLAP_EXT_GUI.as_ptr()).is_null());
            assert!(!get(&raw const host.host, CLAP_EXT_AUDIO_PORTS.as_ptr()).is_null());
            assert!(!get(&raw const host.host, CLAP_EXT_NOTE_PORTS.as_ptr()).is_null());
            let unknown = CString::new("clap.unknown").unwrap();
            assert!(get(&raw const host.host, unknown.as_ptr()).is_null());
        }
    }
}
