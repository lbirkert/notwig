//! CLAP GUI extension implementation for [`ClapPlugin`].
//!
//! Converts the core's platform-neutral [`WindowHandle`] into a `clap_window`
//! and forwards the editor lifecycle to the plugin's `clap.gui` extension.

use crate::plugin::ClapPlugin;

use notwig_rack_core::error::Error;

use clap_sys::ext::gui::{
    CLAP_WINDOW_API_COCOA, CLAP_WINDOW_API_WIN32, CLAP_WINDOW_API_X11, clap_plugin_gui,
    clap_window, clap_window_handle,
};
use clap_sys::plugin::clap_plugin;

use std::ffi::CStr;

/// Platform API id we'd ask the plugin to support — one per
/// build target.
const fn platform_api() -> &'static CStr {
    #[cfg(target_os = "macos")]
    {
        CLAP_WINDOW_API_COCOA
    }
    #[cfg(target_os = "windows")]
    {
        CLAP_WINDOW_API_WIN32
    }
    #[cfg(target_os = "linux")]
    {
        CLAP_WINDOW_API_X11
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        CLAP_WINDOW_API_COCOA
    }
}

pub(crate) unsafe fn gui_supports_current_platform(
    plugin: *const clap_plugin,
    gui_ext: *const clap_plugin_gui,
) -> bool {
    let Some(is_supported) = (unsafe { (*gui_ext).is_api_supported }) else {
        return false;
    };
    unsafe { is_supported(plugin, platform_api().as_ptr(), false) }
}

fn handle_to_clap_window(handle: notwig_rack_core::editor::WindowHandle) -> clap_window {
    use notwig_rack_core::editor::WindowHandle;
    let (api, specific) = match handle {
        WindowHandle::NSView(p) => (
            CLAP_WINDOW_API_COCOA.as_ptr(),
            clap_window_handle { cocoa: p },
        ),
        WindowHandle::HWND(p) => (
            CLAP_WINDOW_API_WIN32.as_ptr(),
            clap_window_handle { win32: p },
        ),
        WindowHandle::X11(id) => (
            CLAP_WINDOW_API_X11.as_ptr(),
            clap_window_handle { x11: id as _ },
        ),
    };
    clap_window { api, specific }
}

impl notwig_rack_core::editor::PluginEditor for ClapPlugin {
    fn open(
        &mut self,
        parent: notwig_rack_core::editor::WindowHandle,
        scale: f64,
    ) -> notwig_rack_core::error::Result<()> {
        if self.gui_open {
            return Ok(());
        }
        if self.gui_ext.is_null() {
            return Err(Error::Other("clap.gui extension absent".into()));
        }
        let api = platform_api();
        let create = unsafe { (*self.gui_ext).create }
            .ok_or_else(|| Error::Other("clap.gui missing `create`".into()))?;
        if !unsafe { create(self.plugin, api.as_ptr(), false) } {
            return Err(Error::Other("clap.gui::create returned false".into()));
        }
        if let Some(set_scale) = unsafe { (*self.gui_ext).set_scale } {
            let _ = unsafe { set_scale(self.plugin, scale) };
        }
        let window = handle_to_clap_window(parent);
        if let Some(set_parent) = unsafe { (*self.gui_ext).set_parent }
            && !unsafe { set_parent(self.plugin, &raw const window) }
        {
            if let Some(destroy) = unsafe { (*self.gui_ext).destroy } {
                unsafe { destroy(self.plugin) };
            }
            return Err(Error::Other("clap.gui::set_parent returned false".into()));
        }
        if let Some(show) = unsafe { (*self.gui_ext).show } {
            let _ = unsafe { show(self.plugin) };
        }
        self.gui_open = true;
        Ok(())
    }

    fn close(&mut self) {
        if !self.gui_open {
            return;
        }
        if let Some(hide) = unsafe { (*self.gui_ext).hide } {
            let _ = unsafe { hide(self.plugin) };
        }
        if let Some(destroy) = unsafe { (*self.gui_ext).destroy } {
            unsafe { destroy(self.plugin) };
        }
        self.gui_open = false;
    }

    fn is_open(&self) -> bool {
        self.gui_open
    }

    fn size(&self) -> Option<(u32, u32)> {
        if !self.gui_open {
            return None;
        }
        let get_size = unsafe { (*self.gui_ext).get_size }?;
        let mut w: u32 = 0;
        let mut h: u32 = 0;
        if unsafe { get_size(self.plugin, &raw mut w, &raw mut h) } {
            Some((w, h))
        } else {
            None
        }
    }

    fn is_resizable(&self) -> bool {
        if !self.gui_open {
            return false;
        }
        unsafe { (*self.gui_ext).can_resize }.is_some_and(|f| unsafe { f(self.plugin) })
    }

    fn set_size(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        if !self.gui_open {
            return None;
        }
        if let Some(can_resize) = unsafe { (*self.gui_ext).can_resize }
            && !unsafe { can_resize(self.plugin) }
        {
            return None;
        }
        let mut w = width;
        let mut h = height;
        if let Some(adjust) = unsafe { (*self.gui_ext).adjust_size } {
            let _ = unsafe { adjust(self.plugin, &raw mut w, &raw mut h) };
        }
        let set = unsafe { (*self.gui_ext).set_size }?;
        if unsafe { set(self.plugin, w, h) } {
            Some((w, h))
        } else {
            None
        }
    }

    fn show(&mut self) {
        if let Some(show) = unsafe { (*self.gui_ext).show } {
            let _ = unsafe { show(self.plugin) };
        }
    }

    fn hide(&mut self) {
        if let Some(hide) = unsafe { (*self.gui_ext).hide } {
            let _ = unsafe { hide(self.plugin) };
        }
    }
}
