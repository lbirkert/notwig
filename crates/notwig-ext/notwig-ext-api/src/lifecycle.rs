//! Extension lifecycle phases and their author-facing builders.
//!
//! The generated ABI calls [`run_preload`], [`run_load`], and [`run_unload`].
//! Guest code normally only sees the builder types as entrypoint arguments.

use std::sync::{Mutex, OnceLock};

use crate::abi::AbiStatus;
use crate::audio_editor::AudioSubEditorExtension;
use crate::command::CommandExtension;
use crate::extension::{DeviceExtension, Extension};
use crate::extension_point::ExtensionPoint;

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
pub fn run_unload(f: impl FnOnce(&mut UnloadContext)) {
    let mut context = UnloadContext::new();
    f(&mut context);
}

/// Builder passed to `#[notwig_ext(preload)]`.
///
/// Only declarations that must exist before load-phase registration are
/// exposed here.
pub struct PreloadBuilder {
    extension: Extension,
}

impl PreloadBuilder {
    /// Creates an empty preload builder.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            extension: Extension::new(),
        }
    }

    /// Declares one extension point.
    pub fn register_extension_point<T: ?Sized>(&mut self, point: ExtensionPoint<T>) {
        self.extension.register_extension_point(point);
    }

    pub(crate) fn into_extension(self) -> Extension {
        self.extension
    }
}

impl Default for PreloadBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder passed to `#[notwig_ext(load)]`.
///
/// This exposes implementation and contribution registration, but not new
/// point declarations.
pub struct ExtensionBuilder {
    extension: Extension,
}

impl ExtensionBuilder {
    /// Creates an empty load builder.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            extension: Extension::new(),
        }
    }

    pub(crate) fn from_extension(extension: Extension) -> Self {
        Self { extension }
    }

    pub(crate) fn into_extension(self) -> Extension {
        self.extension
    }

    /// Registers a typed interface instance on `point`.
    ///
    /// The instance stays inside the guest runtime; only its descriptors cross
    /// the ABI.
    pub fn register_extension<I: ?Sized, T>(&mut self, point: ExtensionPoint<I>, instance: T)
    where
        T: crate::rpc::ExtensionCall + 'static,
    {
        self.extension
            .register_extension_call(point.id(), Box::new(instance));
    }

    /// Registers an implementation factory on `point`.
    ///
    /// This is used by device, command, sub-editor, and bottom-panel
    /// contributions whose host creates separate instances.
    pub fn register_extension_factory<T: ?Sized>(&mut self, point: impl ExtensionPointFor<T>) {
        self.extension.register_extension_factory(point);
    }
}

impl Default for ExtensionBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Registration adapter for a typed extension point.
pub trait ExtensionPointFor<T: ?Sized> {
    /// Registers `T` into the guest runtime.
    fn register(self, extension: &mut Extension);
}

impl<T: DeviceExtension> ExtensionPointFor<T>
    for ExtensionPoint<dyn crate::extension::DeviceInterface>
{
    fn register(self, extension: &mut Extension) {
        extension.register_device_at::<T>(self.id());
    }
}

impl<T: CommandExtension> ExtensionPointFor<T>
    for ExtensionPoint<dyn crate::command::CommandInterface>
{
    fn register(self, extension: &mut Extension) {
        extension.register_command_with_descriptor::<T>(T::descriptor());
    }
}

impl<T: crate::bottom_panel::BottomPanelExtension> ExtensionPointFor<T>
    for ExtensionPoint<dyn crate::bottom_panel::BottomPanelInterface>
{
    fn register(self, extension: &mut Extension) {
        extension.register_bottom_panel_with_descriptor::<T>(T::descriptor());
    }
}

impl<T: AudioSubEditorExtension> ExtensionPointFor<T>
    for ExtensionPoint<dyn crate::audio_editor::AudioSubEditorInterface>
{
    fn register(self, extension: &mut Extension) {
        extension.register_audio_sub_editor_with_descriptor::<T>(T::descriptor());
    }
}

/// Context passed to `#[notwig_ext(unload)]`.
///
/// The host owns instance teardown; this context only marks the phase boundary.
pub struct UnloadContext {
    _private: (),
}

impl UnloadContext {
    /// Creates an unload context.
    #[must_use]
    pub const fn new() -> Self {
        Self { _private: () }
    }
}

impl Default for UnloadContext {
    fn default() -> Self {
        Self::new()
    }
}
