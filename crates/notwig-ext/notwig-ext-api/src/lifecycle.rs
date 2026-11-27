//! Extension lifecycle phases and their author-facing builders.
//!
//! The generated ABI calls [`run_preload`], [`run_load`], and [`run_unload`].
//! Guest code normally only sees the builder types as entrypoint arguments.

use std::sync::{Mutex, OnceLock};

use crate::abi::AbiStatus;
use crate::extension::{Extension, ExtensionBuilder, PreloadBuilder};

static PRELOADED: Mutex<Option<Extension>> = Mutex::new(None);
static LOADED: OnceLock<Extension> = OnceLock::new();

fn take_preloaded() -> Extension {
    PRELOADED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
        .unwrap_or_default()
}

/// Runs the preload phase once.
///
/// `#[notwig_ext(preload)]` uses this to declare extension points before any
/// load-phase registration is collected.
#[doc(hidden)]
pub fn run_preload(f: impl FnOnce(&mut PreloadBuilder)) -> Result<(), AbiStatus> {
    let mut slot = PRELOADED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if slot.is_some() || LOADED.get().is_some() {
        return Err(AbiStatus::Error);
    }
    let mut builder = PreloadBuilder::new();
    f(&mut builder);
    *slot = Some(builder.into_extension());
    Ok(())
}

/// Runs the load phase once, consuming declarations from preload.
///
/// `#[notwig_ext(load)]` uses this to register implementations and existing
/// contributions.
#[doc(hidden)]
pub fn run_load(f: impl FnOnce(&mut ExtensionBuilder)) -> Result<(), AbiStatus> {
    let mut builder = ExtensionBuilder::from_extension(take_preloaded());
    f(&mut builder);
    LOADED
        .set(builder.into_extension())
        .map_err(|_| AbiStatus::Error)
}

/// Ensures the load phase has run and returns the initialized runtime.
///
/// Generated ABI exports use this to support direct invocation without an
/// explicit host load call.
#[doc(hidden)]
pub fn ensure_loaded(f: impl FnOnce(&mut ExtensionBuilder)) -> &'static Extension {
    LOADED.get_or_init(|| {
        let mut builder = ExtensionBuilder::from_extension(take_preloaded());
        f(&mut builder);
        builder.into_extension()
    })
}

/// Runs the unload phase.
#[doc(hidden)]
pub fn run_unload(f: impl FnOnce(&mut crate::extension::UnloadContext)) {
    let mut context = crate::extension::UnloadContext::new();
    f(&mut context);
}
