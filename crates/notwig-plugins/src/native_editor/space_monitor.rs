use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::ptr;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use block2::RcBlock;
use objc2::MainThreadMarker;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2_app_kit::{NSEvent, NSEventMask, NSEventType};

use super::appkit::key_window;

static PLUGIN_WINDOWS: Mutex<BTreeSet<usize>> = Mutex::new(BTreeSet::new());
pub(super) static PLAYBACK_TOGGLE_PENDING: AtomicBool = AtomicBool::new(false);

thread_local! {
    static SPACE_MONITOR: RefCell<Option<Retained<AnyObject>>> =
        const { RefCell::new(None) };
    // Tracks whether the matching Space key-down was swallowed, so the
    // plugin never sees a lone key-up after we consume its key-down.
    static SPACE_KEY_DOWN_SWALLOWED: Cell<bool> = const { Cell::new(false) };
}

const SPACE_KEY_CODE: u16 = 49;

pub(super) fn register_plugin_window(window: *mut AnyObject) {
    let mut windows = PLUGIN_WINDOWS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    windows.insert(window as usize);
    drop(windows);
    install_space_monitor();
}

pub(super) fn unregister_plugin_window(window: *mut AnyObject) {
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
