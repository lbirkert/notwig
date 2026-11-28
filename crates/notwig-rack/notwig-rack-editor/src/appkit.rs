use std::ffi::CString;

use notwig_rack_core::{Error, Result};
use objc2::msg_send;
use objc2::runtime::{AnyClass, AnyObject};
use objc2_foundation::{NSPoint, NSRect, NSSize};

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

fn activate_app() {
    let Some(app_class) = AnyClass::get(c"NSApplication") else {
        return;
    };
    unsafe {
        let app: *mut AnyObject = msg_send![app_class, sharedApplication];
        if !app.is_null() {
            let _: () = msg_send![app, activateIgnoringOtherApps: true];
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

pub(super) unsafe fn create_window(title: &str, width: f64, height: f64) -> Result<*mut AnyObject> {
    // Titled windows keep the standard close, miniaturize, and zoom controls
    // functional. NSWindowStyleMaskBorderless is zero, so never set it.
    const TITLED: usize = 1 << 0;
    const CLOSABLE: usize = 1 << 1;
    const MINIATURIZABLE: usize = 1 << 2;
    const RESIZABLE: usize = 1 << 3;
    let window_class =
        AnyClass::get(c"NSWindow").ok_or_else(|| Error::Other("NSWindow is unavailable".into()))?;
    let rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, height));
    let style = TITLED | CLOSABLE | MINIATURIZABLE | RESIZABLE;
    let allocated: *mut AnyObject = unsafe { msg_send![window_class, alloc] };
    if allocated.is_null() {
        return Err(Error::Other("could not allocate NSWindow".into()));
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
        return Err(Error::Other("could not create NSWindow".into()));
    }
    activate_app();
    unsafe {
        let _: () = msg_send![window, setReleasedWhenClosed: false];
        let _: () = msg_send![window, setLevel: 3isize];
        let _: () = msg_send![window, setHidesOnDeactivate: false];
    }
    if let Ok(title) = CString::new(title)
        && let Some(string_class) = AnyClass::get(c"NSString")
    {
        let ns_title: *mut AnyObject =
            unsafe { msg_send![string_class, stringWithUTF8String: title.as_ptr()] };
        if !ns_title.is_null() {
            unsafe {
                let _: () = msg_send![window, setTitle: ns_title];
            }
        }
    }
    Ok(window)
}

/// Render the window's content view to PNG bytes.
///
/// Uses `NSBitmapImageRep`'s caching-display path so the capture does not
/// require screen-recording permission. Some GPU/layer-backed plugin editors
/// may not draw into the offscreen bitmap; callers should treat a blank image
/// as a capture limitation rather than a plugin failure.
pub(super) unsafe fn capture_window_png(window: *mut AnyObject) -> Result<Vec<u8>> {
    // NSBitmapImageFileTypePNG is 4 on every supported macOS release.
    const NS_BITMAP_IMAGE_FILE_TYPE_PNG: usize = 4;
    if window.is_null() {
        return Err(Error::Other("cannot capture a null window".into()));
    }
    let content: *mut AnyObject = unsafe { msg_send![window, contentView] };
    if content.is_null() {
        return Err(Error::Other("window has no content view".into()));
    }
    let bounds: NSRect = unsafe { msg_send![content, bounds] };
    let rep: *mut AnyObject =
        unsafe { msg_send![content, bitmapImageRepForCachingDisplayInRect: bounds] };
    if rep.is_null() {
        return Err(Error::Other(
            "plugin view could not create a bitmap image representation".into(),
        ));
    }
    unsafe {
        let _: () = msg_send![content, cacheDisplayInRect: bounds, toBitmapImageRep: rep];
    }
    let Some(dictionary_class) = AnyClass::get(c"NSDictionary") else {
        return Err(Error::Other("NSDictionary is unavailable".into()));
    };
    let properties: *mut AnyObject = unsafe { msg_send![dictionary_class, dictionary] };
    if properties.is_null() {
        return Err(Error::Other(
            "could not allocate an empty image properties dictionary".into(),
        ));
    }
    let data: *mut AnyObject = unsafe {
        msg_send![
            rep,
            representationUsingType: NS_BITMAP_IMAGE_FILE_TYPE_PNG,
            properties: properties
        ]
    };
    if data.is_null() {
        return Err(Error::Other(
            "plugin view could not encode its bitmap as PNG".into(),
        ));
    }
    let length: usize = unsafe { msg_send![data, length] };
    let bytes: *const u8 = unsafe { msg_send![data, bytes] };
    if length == 0 || bytes.is_null() {
        return Err(Error::Other("plugin view produced an empty PNG".into()));
    }
    let png = unsafe { std::slice::from_raw_parts(bytes, length) }.to_vec();
    Ok(png)
}
