//! `catch_unwind` helpers for FFI boundaries between rack and
//! plugin code.
//!
//! Plugin libraries are written by third parties. A panic in a
//! plugin's `process` (or any other callback) would unwind
//! across the `extern "C"` boundary back into the host —
//! undefined behaviour on most toolchains, abort on others.
//! These helpers catch the unwind, record a lock-free error flag,
//! and return a fallback value the wrapper can hand back to its
//! caller. They never log or allocate, so they are safe on the
//! audio thread; the host can poll [`take_audio_panic`] and surface
//! the deferred diagnostic through its own status channel.
//!
//! Mirrors `truce_core::wrapper`. The plugin-side framework
//! catches panics going *out* of plugin code; the host-side
//! framework catches panics going *in* from plugin code. Same
//! helper shape, opposite direction.

use crate::error::Error;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Set when any audio/FFI callback panic is caught. The host can
/// swap it back to `false` with [`take_audio_panic`].
static AUDIO_PANIC_PENDING: AtomicBool = AtomicBool::new(false);
/// Monotonic count of caught callback panics for diagnostics.
static AUDIO_PANIC_COUNT: AtomicU32 = AtomicU32::new(0);

fn record_audio_panic() {
    AUDIO_PANIC_COUNT.fetch_add(1, Ordering::Relaxed);
    AUDIO_PANIC_PENDING.store(true, Ordering::Release);
}

/// Take the pending panic flag and reset it.
///
/// Returns `true` if at least one callback panic was caught since the
/// previous call. Lock-free and allocation-free.
#[must_use]
pub fn take_audio_panic() -> bool {
    AUDIO_PANIC_PENDING.swap(false, Ordering::AcqRel)
}

/// Total number of callback panics caught since process start.
#[must_use]
pub fn audio_panic_count() -> u32 {
    AUDIO_PANIC_COUNT.load(Ordering::Relaxed)
}

/// Run a per-audio-block callback body under [`catch_unwind`]
/// with no fallback value — caller only cares whether the body
/// panicked.
///
/// Returns `true` on a clean exit, `false` on panic. Wrappers
/// should zero output buffers on `false` so the host doesn't
/// hear garbage from whatever was in those slots.
#[must_use]
pub fn run_audio_block<P>(format: &str, body: impl FnOnce()) -> bool {
    let _ = (format, std::marker::PhantomData::<P>);
    let result = catch_unwind(AssertUnwindSafe(body));
    if result.is_err() {
        record_audio_panic();
        return false;
    }
    true
}

/// Run a per-audio-block callback body under [`catch_unwind`]
/// with a fallback return value. Returns the body's value on
/// clean exit, `fallback` on panic.
pub fn run_audio_block_with<P, R>(format: &str, fallback: R, body: impl FnOnce() -> R) -> R {
    let _ = (format, std::marker::PhantomData::<P>);
    if let Ok(value) = catch_unwind(AssertUnwindSafe(body)) {
        value
    } else {
        record_audio_panic();
        fallback
    }
}

/// Run an `extern "C"` plugin callback body (state save / load,
/// parameter formatting, GUI handler) under `catch_unwind`
/// with a fallback return value. A panic records the lock-free
/// audio-panic flag (see [`take_audio_panic`]) instead of logging.
pub fn run_extern_callback_with<P, R>(
    format: &str,
    action: &str,
    fallback: R,
    body: impl FnOnce() -> R,
) -> R {
    let _ = (format, action, std::marker::PhantomData::<P>);
    if let Ok(value) = catch_unwind(AssertUnwindSafe(body)) {
        value
    } else {
        record_audio_panic();
        fallback
    }
}

/// Convenience: run a plugin callback under [`catch_unwind`]
/// and convert a panic into an [`Error::Panic`]. Used by the
/// `PluginCore` / `Plugin` impl shims when the callback's
/// natural error type is `crate::Result<T>`.
///
/// Distinct from [`run_extern_callback_with`] in that this one
/// is *inside* the rack-side Rust trait surface, not at the
/// raw C ABI edge — but the catch-and-log pattern is the same.
///
/// # Errors
/// `body`'s error propagates on a clean failure; a panic in
/// `body` returns [`Error::Panic`] with the supplied `action`
/// string.
pub fn run_callable<P, T>(
    action: &'static str,
    body: impl FnOnce() -> crate::Result<T>,
) -> crate::Result<T> {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(result) => result,
        Err(payload) => Err(Error::Panic {
            action,
            message: extract_panic_msg(&payload).to_string(),
        }),
    }
}

fn extract_panic_msg(payload: &Box<dyn std::any::Any + Send>) -> &str {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        s
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.as_str()
    } else {
        "<non-string panic payload>"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dummy;

    #[test]
    fn clean_audio_block_returns_true() {
        let ran = std::cell::Cell::new(false);
        let result = run_audio_block::<Dummy>("test", || ran.set(true));
        assert!(result);
        assert!(ran.get());
    }

    #[test]
    fn panicking_audio_block_returns_false() {
        let result = run_audio_block::<Dummy>("test", || panic!("boom"));
        assert!(!result);
    }

    #[test]
    fn panicking_audio_block_records_lock_free_flag() {
        let _ = take_audio_panic();
        let before = audio_panic_count();
        let result = run_audio_block_with::<Dummy, i32>("test", -1, || panic!("boom"));
        assert_eq!(result, -1);
        assert!(take_audio_panic());
        assert!(audio_panic_count() > before);
    }

    #[test]
    fn callable_panic_becomes_error() {
        let result: crate::Result<()> = run_callable::<Dummy, ()>("save_state", || panic!("oops"));
        match result {
            Err(Error::Panic { action, .. }) => assert_eq!(action, "save_state"),
            _ => panic!("expected Error::Panic"),
        }
    }

    #[test]
    fn callable_error_propagates() {
        let result: crate::Result<()> =
            run_callable::<Dummy, ()>("save_state", || Err(Error::Other("plain error".into())));
        assert!(matches!(result, Err(Error::Other(_))));
    }
}
