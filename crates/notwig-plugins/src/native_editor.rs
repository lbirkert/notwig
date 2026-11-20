#![allow(unsafe_code)]

#[cfg(not(target_os = "macos"))]
use crate::types::{PluginDescriptor, PluginError, PluginFormat};

#[cfg(target_os = "macos")]
mod imp {
    use std::collections::BTreeMap;
    use std::ffi::CString;
    use std::ptr;
    use std::time::{Duration, Instant};

    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    use truce_rack::core::{BusLayout, Plugin, WindowHandle};

    use crate::types::{PluginDescriptor, PluginError, PluginFormat, PluginState};

    pub struct NativePluginWindow {
        plugin: Box<dyn Plugin<f32>>,
        format: PluginFormat,
        plugin_uid: String,
        plugin_version: String,
        window: *mut AnyObject,
        open: bool,
        params: BTreeMap<u32, usize>,
        last_values: BTreeMap<u32, f64>,
        last_poll: Instant,
    }

    impl NativePluginWindow {
        pub fn open(descriptor: &PluginDescriptor) -> Result<Self, PluginError> {
            let mut plugin = crate::adapters::load_plugin(descriptor)?;
            let _ = plugin.activate(BusLayout::stereo(), 48_000.0, 512);
            let mut params = BTreeMap::new();
            let mut last_values = BTreeMap::new();
            for index in 0..plugin.parameter_count() {
                if let Ok(info) = plugin.parameter_info(index) {
                    params.insert(info.id, index);
                    if let Ok(value) = plugin.parameter_value(index) {
                        last_values.insert(info.id, value);
                    }
                }
            }
            let window = unsafe { create_window(&descriptor.name, 760.0, 560.0) }?;
            let content: *mut AnyObject = unsafe { msg_send![window, contentView] };
            if content.is_null() {
                unsafe {
                    let _: () = msg_send![window, release];
                }
                return Err(PluginError::Load(
                    "plugin window has no content view".into(),
                ));
            }
            let Some(editor) = plugin.editor() else {
                unsafe {
                    let _: () = msg_send![window, release];
                }
                return Err(PluginError::Load("plugin has no editor".into()));
            };
            editor
                .open(WindowHandle::NSView(content.cast()), 1.0)
                .map_err(|err| PluginError::Load(err.to_string()))?;
            if let Some((width, height)) = editor.size() {
                let _ = editor.set_size(width, height);
                let size = NSSize::new(f64::from(width), f64::from(height));
                unsafe {
                    let _: () = msg_send![window, setContentSize: size];
                }
            }
            editor.show();
            unsafe {
                let _: () = msg_send![window, center];
                let _: () = msg_send![window, makeKeyAndOrderFront: ptr::null_mut::<AnyObject>()];
            }
            Ok(Self {
                plugin,
                format: descriptor.format,
                plugin_uid: descriptor.uid.clone(),
                plugin_version: descriptor.version.clone(),
                window,
                open: true,
                params,
                last_values,
                last_poll: Instant::now(),
            })
        }

        pub fn focus(&self) {
            if self.open {
                unsafe {
                    let _: () =
                        msg_send![self.window, makeKeyAndOrderFront: ptr::null_mut::<AnyObject>()];
                }
            }
        }

        pub fn pump(&mut self) {
            if !self.open {
                return;
            }
            let visible: bool = unsafe { msg_send![self.window, isVisible] };
            if !visible {
                self.close();
                return;
            }
            if let Some(editor) = self.plugin.editor() {
                editor.on_idle();
            }
        }

        pub fn is_open(&self) -> bool {
            self.open
        }

        pub fn set_parameter(&mut self, id: u32, value: f64) {
            if let Some(index) = self.params.get(&id).copied() {
                let _ = self.plugin.set_parameter(index, value);
                self.last_values.insert(id, value);
            }
        }

        pub fn save_state(&mut self) -> Result<PluginState, PluginError> {
            let blob = self
                .plugin
                .save_state()
                .map_err(|err| PluginError::Load(err.to_string()))?;
            Ok(PluginState {
                format: self.format,
                plugin_uid: self.plugin_uid.clone(),
                plugin_version: self.plugin_version.clone(),
                blob,
            })
        }

        pub fn load_state(&mut self, state: &PluginState) -> Result<(), PluginError> {
            if state.format != self.format || state.plugin_uid != self.plugin_uid {
                return Err(PluginError::StateIncompatible(
                    "plugin state does not match the open editor".into(),
                ));
            }
            self.plugin
                .load_state(&state.blob)
                .map_err(|err| PluginError::Load(err.to_string()))
        }

        pub fn poll_changes(&mut self) -> Vec<(u32, f64)> {
            if self.last_poll.elapsed() < Duration::from_millis(50) {
                return Vec::new();
            }
            self.last_poll = Instant::now();
            let mut changes = Vec::new();
            for (id, index) in &self.params {
                let Ok(value) = self.plugin.parameter_value(*index) else {
                    continue;
                };
                let differs = self
                    .last_values
                    .get(id)
                    .is_none_or(|previous| (previous - value).abs() > 1.0e-4);
                if differs {
                    changes.push((*id, value));
                    self.last_values.insert(*id, value);
                }
            }
            changes
        }

        fn close(&mut self) {
            if !self.open {
                return;
            }
            if let Some(editor) = self.plugin.editor() {
                editor.close();
            }
            unsafe {
                let _: () = msg_send![self.window, orderOut: ptr::null_mut::<AnyObject>()];
                let _: () = msg_send![self.window, release];
            }
            self.window = ptr::null_mut();
            self.open = false;
        }
    }

    impl Drop for NativePluginWindow {
        fn drop(&mut self) {
            self.close();
        }
    }

    unsafe fn create_window(
        title: &str,
        width: f64,
        height: f64,
    ) -> Result<*mut AnyObject, PluginError> {
        let window_class = AnyClass::get(c"NSWindow")
            .ok_or_else(|| PluginError::Load("NSWindow is unavailable".into()))?;
        let rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, height));
        let style: usize = 1 | 2 | 4 | 8;
        let allocated: *mut AnyObject = unsafe { msg_send![window_class, alloc] };
        if allocated.is_null() {
            return Err(PluginError::Load("could not allocate NSWindow".into()));
        }
        let window: *mut AnyObject = unsafe {
            msg_send![
                allocated,
                initWithContentRect: rect,
                styleMask: style,
                backing: 2usize,
                defer: false
            ]
        };
        if window.is_null() {
            unsafe {
                let _: () = msg_send![allocated, release];
            }
            return Err(PluginError::Load("could not create NSWindow".into()));
        }
        unsafe {
            let _: () = msg_send![window, setReleasedWhenClosed: false];
            let _: () = msg_send![window, setLevel: 3isize];
            let _: () = msg_send![window, setHidesOnDeactivate: false];
        }
        if let Ok(title) = CString::new(title) {
            if let Some(string_class) = AnyClass::get(c"NSString") {
                let ns_title: *mut AnyObject =
                    unsafe { msg_send![string_class, stringWithUTF8String: title.as_ptr()] };
                if !ns_title.is_null() {
                    unsafe {
                        let _: () = msg_send![window, setTitle: ns_title];
                    }
                }
            }
        }
        Ok(window)
    }
}

#[cfg(target_os = "macos")]
pub use imp::NativePluginWindow;

#[cfg(not(target_os = "macos"))]
pub struct NativePluginWindow;

#[cfg(not(target_os = "macos"))]
impl NativePluginWindow {
    pub fn open(descriptor: &PluginDescriptor) -> Result<Self, PluginError> {
        Err(PluginError::BackendUnavailable(descriptor.format))
    }

    pub fn focus(&self) {}

    pub fn pump(&mut self) {}

    pub fn is_open(&self) -> bool {
        false
    }

    pub fn set_parameter(&mut self, _id: u32, _value: f64) {}

    pub fn poll_changes(&mut self) -> Vec<(u32, f64)> {
        Vec::new()
    }
}
