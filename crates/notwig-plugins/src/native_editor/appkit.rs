use std::ffi::CString;

use objc2::msg_send;
use objc2::runtime::{AnyClass, AnyObject};
use objc2_foundation::{NSPoint, NSRect, NSSize};

use crate::types::PluginError;

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
        let date: *mut AnyObject = msg_send![date_class, dateWithTimeIntervalSinceNow: 0.008_f64];
        if !run_loop.is_null() && !date.is_null() {
            let _: () = msg_send![run_loop, runUntilDate: date];
        }
    }
}

pub(super) fn key_window() -> *mut AnyObject {
    let Some(app_class) = AnyClass::get(c"NSApplication") else {
        return std::ptr::null_mut();
    };
    let app: *mut AnyObject = unsafe { msg_send![app_class, sharedApplication] };
    if app.is_null() {
        return std::ptr::null_mut();
    }
    unsafe { msg_send![app, keyWindow] }
}

pub(super) unsafe fn release_window(window: *mut AnyObject) {
    if !window.is_null() {
        unsafe {
            let _: () = msg_send![window, release];
        }
    }
}

pub(super) unsafe fn lock_window_size(
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

pub(super) unsafe fn create_window(
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
