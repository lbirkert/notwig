//! Host state and the `notwig.host` imports visible to WASM extensions.

use wasmtime::{Caller, Linker, StoreLimits};

use crate::error::{ExtensionError, wasm_err};

/// Upper bound for one `log`/`notify` message, including invalid input.
///
/// Guest logging is best-effort diagnostics, not a data channel. Capping the
/// copy keeps a guest from spending an unbounded amount of host time or stack
/// space in an import call and keeps the audio path allocation-free when no
/// tracing subscriber is interested in the message.
const MAX_HOST_LOG_BYTES: usize = 8 * 1024;

pub(super) struct HostState {
    pub(super) extension_id: String,
    pub(super) limits: StoreLimits,
}

pub(super) fn install(linker: &mut Linker<HostState>) -> Result<(), ExtensionError> {
    linker
        .func_wrap(
            "notwig.host",
            "log",
            |mut caller: Caller<'_, HostState>, level: i32, ptr: i32, len: i32| {
                let level = match level {
                    0 => tracing::Level::DEBUG,
                    1 => tracing::Level::INFO,
                    _ => tracing::Level::WARN,
                };
                if !tracing::level_enabled!(level) {
                    return;
                }
                with_bounded_text(&mut caller, ptr, len, |text, truncated, state| {
                    let extension = state.extension_id.as_str();
                    match level {
                        tracing::Level::DEBUG => {
                            tracing::debug!(extension, truncated, "{text}");
                        }
                        tracing::Level::INFO => {
                            tracing::info!(extension, truncated, "{text}");
                        }
                        _ => {
                            tracing::warn!(extension, truncated, "{text}");
                        }
                    }
                });
            },
        )
        .map_err(wasm_err)?;
    linker
        .func_wrap(
            "notwig.host",
            "notify",
            |mut caller: Caller<'_, HostState>, ptr: i32, len: i32| {
                if !tracing::enabled!(tracing::Level::INFO) {
                    return;
                }
                with_bounded_text(&mut caller, ptr, len, |text, truncated, state| {
                    let extension = state.extension_id.as_str();
                    tracing::info!(extension, truncated, "{text}");
                });
            },
        )
        .map_err(wasm_err)?;
    Ok(())
}

/// Borrows a capped, lossily decoded view of guest text without allocating for
/// valid UTF-8. `truncated` reports that the guest message exceeded the cap.
fn with_bounded_text<R>(
    caller: &mut Caller<'_, HostState>,
    ptr: i32,
    len: i32,
    f: impl FnOnce(&str, bool, &HostState) -> R,
) -> Option<R> {
    let memory = caller.get_export("memory")?.into_memory()?;
    let start = usize::try_from(ptr).ok()?;
    let len = usize::try_from(len).ok()?;
    let end = start.checked_add(len)?;
    let data = memory.data(&*caller);
    if end > data.len() {
        return None;
    }
    let kept = len.min(MAX_HOST_LOG_BYTES);
    let mut buffer = [0_u8; MAX_HOST_LOG_BYTES];
    buffer[..kept].copy_from_slice(&data[start..start + kept]);
    let text = String::from_utf8_lossy(&buffer[..kept]);
    Some(f(&text, kept < len, caller.data()))
}
