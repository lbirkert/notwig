#![allow(unsafe_code)]

#[cfg(not(target_os = "macos"))]
use crate::types::{PluginDescriptor, PluginError, PluginState};

#[cfg(target_os = "macos")]
mod imp {
    use std::cell::{Cell, RefCell};
    use std::collections::{BTreeMap, BTreeSet};
    use std::ffi::CString;
    use std::ptr;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use block2::RcBlock;
    use notwig_core::dsp::{AudioMonitor, MonitorBlock};
    use notwig_rack::core::buffer::BusRange;
    use notwig_rack::core::{
        AudioBuffer, BusLayout, EventList, Plugin, ProcessContext as RackContext, WindowHandle,
    };
    use objc2::MainThreadMarker;
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2_app_kit::{NSEvent, NSEventMask, NSEventType};
    use objc2_foundation::{NSPoint, NSRect, NSSize};

    use crate::types::{PluginDescriptor, PluginError, PluginFormat, PluginState};

    static PLUGIN_WINDOWS: Mutex<BTreeSet<usize>> = Mutex::new(BTreeSet::new());
    static PLAYBACK_TOGGLE_PENDING: AtomicBool = AtomicBool::new(false);

    thread_local! {
        static SPACE_MONITOR: RefCell<Option<Retained<AnyObject>>> =
            const { RefCell::new(None) };
        // Tracks whether the matching Space key-down was swallowed, so the
        // plugin never sees a lone key-up after we consume its key-down.
        static SPACE_KEY_DOWN_SWALLOWED: Cell<bool> = const { Cell::new(false) };
    }

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
        window_size: (u32, u32),
        monitor: Option<Arc<AudioMonitor>>,
        sample_rate: f64,
        monitor_input: [Vec<f32>; 2],
        monitor_output: [Vec<f32>; 2],
    }

    impl NativePluginWindow {
        pub fn open(
            descriptor: &PluginDescriptor,
            sample_rate: u32,
            monitor: Option<Arc<AudioMonitor>>,
        ) -> Result<Self, PluginError> {
            let mut plugin = crate::adapters::load_plugin(descriptor)?;
            let sample_rate = f64::from(sample_rate.max(1));
            let _ = plugin.activate(BusLayout::stereo(), sample_rate, 512);
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
                    release_window(window);
                }
                return Err(PluginError::Load(
                    "plugin window has no content view".into(),
                ));
            }
            let Some(editor) = plugin.editor() else {
                unsafe {
                    release_window(window);
                }
                return Err(PluginError::Load("plugin has no editor".into()));
            };
            if let Err(err) = editor.open(WindowHandle::NSView(content.cast()), 1.0) {
                editor.close();
                unsafe {
                    release_window(window);
                }
                return Err(PluginError::Load(err.to_string()));
            }
            let (plugin_size, resizable) = (
                editor.size().filter(|(w, h)| *w > 0 && *h > 0),
                editor.is_resizable(),
            );
            editor.show();
            let window_size = plugin_size.unwrap_or((760, 560));
            unsafe {
                lock_window_size(window, window_size, resizable);
            }
            unsafe {
                let _: () = msg_send![window, center];
                let _: () = msg_send![window, makeKeyAndOrderFront: ptr::null_mut::<AnyObject>()];
            }
            register_plugin_window(window);
            if let Some(monitor) = &monitor {
                monitor.set_enabled(true);
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
                window_size,
                monitor,
                sample_rate,
                monitor_input: std::array::from_fn(|_| vec![0.0; 512]),
                monitor_output: std::array::from_fn(|_| vec![0.0; 512]),
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
            let (plugin_size, resizable) = if let Some(editor) = self.plugin.editor() {
                editor.on_idle();
                (
                    editor.size().filter(|(w, h)| *w > 0 && *h > 0),
                    editor.is_resizable(),
                )
            } else {
                (None, false)
            };
            if let Some(size) = plugin_size
                && size != self.window_size
            {
                unsafe {
                    lock_window_size(self.window, size, resizable);
                }
                self.window_size = size;
            }
            self.drain_monitor();
        }

        fn drain_monitor(&mut self) {
            let Some(monitor) = self.monitor.clone() else {
                return;
            };
            while let Some(block) = monitor.pop() {
                self.process_monitor_block(&block);
            }
        }

        fn process_monitor_block(&mut self, block: &MonitorBlock) {
            let frames = block.frames.min(512);
            if frames == 0 {
                return;
            }
            for channel in &mut self.monitor_input {
                if channel.len() < frames {
                    channel.resize(frames, 0.0);
                }
            }
            for channel in &mut self.monitor_output {
                if channel.len() < frames {
                    channel.resize(frames, 0.0);
                }
                channel[..frames].fill(0.0);
            }
            for frame in 0..frames {
                self.monitor_input[0][frame] = block.samples[frame * 2];
                self.monitor_input[1][frame] = block.samples[frame * 2 + 1];
            }

            let input_refs: [&[f32]; 2] = [
                &self.monitor_input[0][..frames],
                &self.monitor_input[1][..frames],
            ];
            let (left, right) = self.monitor_output.split_at_mut(1);
            let mut output_refs: [&mut [f32]; 2] =
                [&mut left[0][..frames], &mut right[0][..frames]];
            let buses = [BusRange::new(0, 2)];
            let mut buffer =
                AudioBuffer::new(&input_refs, &mut output_refs, frames, &buses, &buses);
            let events = EventList::new();
            let mut output_events = EventList::new();
            let mut context = RackContext {
                sample_rate: self.sample_rate,
                max_block_size: 512,
                transport: None,
                output_events: &mut output_events,
            };
            let _ = self.plugin.process(&mut buffer, &events, &mut context);
        }

        pub fn is_open(&self) -> bool {
            self.open
        }

        /// Returns and clears a playback-toggle request recorded while a plugin
        /// window was key. The Space key that triggered it is swallowed by the
        /// event monitor and never reaches the plugin editor.
        pub fn take_playback_toggle(&mut self) -> bool {
            PLAYBACK_TOGGLE_PENDING.swap(false, Ordering::AcqRel)
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
            if let Some(monitor) = &self.monitor {
                monitor.set_enabled(false);
            }
            unregister_plugin_window(self.window);
            unsafe {
                let _: () = msg_send![self.window, orderOut: ptr::null_mut::<AnyObject>()];
                release_window(self.window);
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

    pub fn prepare_window_host() {
        let Some(app_class) = AnyClass::get(c"NSApplication") else {
            return;
        };
        unsafe {
            let app: *mut AnyObject = msg_send![app_class, sharedApplication];
            if app.is_null() {
                return;
            }
            // Accessory keeps the helper out of the Dock while still allowing
            // plugin windows to become key and receive events.
            let _: bool = msg_send![app, setActivationPolicy: 1isize];
            let _: () = msg_send![app, finishLaunching];
        }
    }

    pub fn pump_window_host() {
        let Some(run_loop_class) = AnyClass::get(c"NSRunLoop") else {
            return;
        };
        let Some(date_class) = AnyClass::get(c"NSDate") else {
            return;
        };
        unsafe {
            let run_loop: *mut AnyObject = msg_send![run_loop_class, currentRunLoop];
            let date: *mut AnyObject =
                msg_send![date_class, dateWithTimeIntervalSinceNow: 0.008_f64];
            if !run_loop.is_null() && !date.is_null() {
                let _: () = msg_send![run_loop, runUntilDate: date];
            }
        }
    }

    const SPACE_KEY_CODE: u16 = 49;

    fn register_plugin_window(window: *mut AnyObject) {
        let mut windows = PLUGIN_WINDOWS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        windows.insert(window as usize);
        drop(windows);
        install_space_monitor();
    }

    fn unregister_plugin_window(window: *mut AnyObject) {
        let empty = {
            let mut windows = PLUGIN_WINDOWS
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            windows.remove(&(window as usize));
            windows.is_empty()
        };
        if empty {
            remove_space_monitor();
        }
    }

    fn install_space_monitor() {
        if MainThreadMarker::new().is_none() {
            return;
        }
        SPACE_MONITOR.with(|slot| {
            if slot.borrow().is_some() {
                return;
            }
            let block = RcBlock::new(|event: std::ptr::NonNull<NSEvent>| -> *mut NSEvent {
                let event_ptr = event.as_ptr();
                unsafe {
                    if record_space_toggle(event_ptr) {
                        SPACE_KEY_DOWN_SWALLOWED.with(|swallowed| swallowed.set(true));
                        return ptr::null_mut();
                    }
                    if swallow_owned_space_key_up(event_ptr) {
                        return ptr::null_mut();
                    }
                }
                event_ptr
            });
            let monitor = unsafe {
                NSEvent::addLocalMonitorForEventsMatchingMask_handler(
                    NSEventMask::KeyDown | NSEventMask::KeyUp,
                    &block,
                )
            };
            if let Some(monitor) = monitor {
                *slot.borrow_mut() = Some(monitor);
            }
        });
    }

    fn remove_space_monitor() {
        if MainThreadMarker::new().is_none() {
            return;
        }
        SPACE_MONITOR.with(|slot| {
            if let Some(monitor) = slot.borrow_mut().take() {
                unsafe {
                    NSEvent::removeMonitor(&monitor);
                }
            }
        });
    }

    /// Records a transport toggle for a Space key-down aimed at a plugin window.
    ///
    /// Returns `true` when the event belongs to the plugin-window transport
    /// shortcut and must be swallowed by the monitor. Key repeats are swallowed
    /// but do not toggle transport again. Events whose first responder is a
    /// text view or field editor return `false` so spaces can be typed into
    /// plugin text fields.
    unsafe fn record_space_toggle(event: *mut NSEvent) -> bool {
        let Some(event) = (unsafe { event.as_ref() }) else {
            return false;
        };
        if event.r#type() != NSEventType::KeyDown || event.keyCode() != SPACE_KEY_CODE {
            return false;
        }
        let event_window: *mut AnyObject = unsafe { msg_send![event, window] };
        let window = if is_plugin_window(event_window) {
            event_window
        } else {
            let key_window = key_window();
            if !is_plugin_window(key_window) {
                return false;
            }
            key_window
        };
        if first_responder_accepts_text_input(window) {
            // A new key-down supersedes any key-up ownership we still held.
            SPACE_KEY_DOWN_SWALLOWED.with(|swallowed| swallowed.set(false));
            return false;
        }
        if !event.isARepeat() {
            PLAYBACK_TOGGLE_PENDING.store(true, Ordering::Release);
        }
        true
    }

    /// Swallows the Space key-up paired with a key-down we already consumed.
    unsafe fn swallow_owned_space_key_up(event: *mut NSEvent) -> bool {
        let Some(event) = (unsafe { event.as_ref() }) else {
            return false;
        };
        if event.r#type() != NSEventType::KeyUp || event.keyCode() != SPACE_KEY_CODE {
            return false;
        }
        SPACE_KEY_DOWN_SWALLOWED.with(|swallowed| swallowed.replace(false))
    }

    /// True when the window's first responder is an `AppKit` text view or field
    /// editor, which must keep receiving spaces.
    fn first_responder_accepts_text_input(window: *mut AnyObject) -> bool {
        if window.is_null() {
            return false;
        }
        let responder: *mut AnyObject = unsafe { msg_send![window, firstResponder] };
        is_text_input_responder(responder)
    }

    fn is_text_input_responder(responder: *mut AnyObject) -> bool {
        if responder.is_null() {
            return false;
        }
        // `NSText` covers field editors and `NSTextView` subclasses without
        // requiring their AppKit features to be enabled in this crate.
        let Some(text_class) = AnyClass::get(c"NSText") else {
            return false;
        };
        let mut class = unsafe { (*responder).class() };
        loop {
            if ptr::eq(class, text_class) {
                return true;
            }
            match class.superclass() {
                Some(superclass) => class = superclass,
                None => return false,
            }
        }
    }

    fn is_plugin_window(window: *mut AnyObject) -> bool {
        if window.is_null() {
            return false;
        }
        PLUGIN_WINDOWS
            .lock()
            .is_ok_and(|windows| windows.contains(&(window as usize)))
    }

    fn key_window() -> *mut AnyObject {
        let Some(app_class) = AnyClass::get(c"NSApplication") else {
            return ptr::null_mut();
        };
        let app: *mut AnyObject = unsafe { msg_send![app_class, sharedApplication] };
        if app.is_null() {
            return ptr::null_mut();
        }
        unsafe { msg_send![app, keyWindow] }
    }

    unsafe fn release_window(window: *mut AnyObject) {
        if !window.is_null() {
            unsafe {
                let _: () = msg_send![window, release];
            }
        }
    }

    unsafe fn lock_window_size(
        window: *mut AnyObject,
        (width, height): (u32, u32),
        resizable: bool,
    ) {
        let size = NSSize::new(f64::from(width), f64::from(height));
        unsafe {
            let _: () = msg_send![window, setContentSize: size];
            if !resizable {
                let _: () = msg_send![window, setContentMinSize: size];
                let _: () = msg_send![window, setContentMaxSize: size];
            }
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
pub use imp::{NativePluginWindow, prepare_window_host, pump_window_host};

#[cfg(not(target_os = "macos"))]
pub fn prepare_window_host() {}

#[cfg(not(target_os = "macos"))]
pub fn pump_window_host() {}

#[cfg(not(target_os = "macos"))]
pub struct NativePluginWindow;

#[cfg(not(target_os = "macos"))]
impl NativePluginWindow {
    pub fn open(
        descriptor: &PluginDescriptor,
        _sample_rate: u32,
        _monitor: Option<std::sync::Arc<notwig_core::dsp::AudioMonitor>>,
    ) -> Result<Self, PluginError> {
        Err(PluginError::BackendUnavailable(descriptor.format))
    }

    pub fn focus(&self) {}

    pub fn pump(&mut self) {}

    pub fn is_open(&self) -> bool {
        false
    }

    pub fn take_playback_toggle(&mut self) -> bool {
        false
    }

    pub fn set_parameter(&mut self, _id: u32, _value: f64) {}

    pub fn poll_changes(&mut self) -> Vec<(u32, f64)> {
        Vec::new()
    }

    pub fn save_state(&mut self) -> Result<PluginState, PluginError> {
        Err(PluginError::Load(
            "native plugin editors are unavailable on this platform".into(),
        ))
    }

    pub fn load_state(&mut self, _state: &PluginState) -> Result<(), PluginError> {
        Err(PluginError::Load(
            "native plugin editors are unavailable on this platform".into(),
        ))
    }
}
