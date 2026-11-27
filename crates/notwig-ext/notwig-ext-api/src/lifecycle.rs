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

    /// Registers a device implementation on `point`.
    pub fn register_extension<D: DeviceExtension>(
        &mut self,
        point: ExtensionPoint<dyn crate::extension::DeviceInterface>,
    ) {
        self.extension.register_device_at::<D>(point.id());
    }

    /// Registers one device implementation on [`DEVICE_POINT`](crate::DEVICE_POINT).
    pub fn register_device<D: DeviceExtension>(&mut self) {
        self.extension.register_device::<D>();
    }

    /// Registers one extension-drawn audio sub-editor renderer.
    pub fn register_audio_sub_editor<T: AudioSubEditorExtension>(&mut self, id: impl Into<String>) {
        self.extension.register_audio_sub_editor::<T>(id);
    }

    /// Registers an audio sub-editor renderer on `point` with runtime metadata.
    pub fn register_extension_audio_sub_editor<T: AudioSubEditorExtension>(
        &mut self,
        _point: ExtensionPoint<dyn crate::audio_editor::AudioSubEditorInterface>,
        descriptor: crate::audio_editor::AudioSubEditorDescriptor,
    ) {
        self.extension
            .register_audio_sub_editor_with_descriptor::<T>(descriptor);
    }

    /// Registers an audio sub-editor renderer with runtime metadata.
    pub fn register_audio_sub_editor_with_descriptor<T: AudioSubEditorExtension>(
        &mut self,
        descriptor: crate::audio_editor::AudioSubEditorDescriptor,
    ) {
        self.register_extension_audio_sub_editor::<T>(
            crate::audio_editor::AUDIO_SUB_EDITOR_EXTENSION_POINT,
            descriptor,
        );
    }

    /// Registers a command implementation under `id`.
    pub fn register_command<C: CommandExtension>(&mut self, id: impl Into<String>) {
        self.extension.register_command::<C>(id);
    }

    /// Registers a command implementation on `point` with runtime metadata.
    pub fn register_extension_command<C: CommandExtension>(
        &mut self,
        _point: ExtensionPoint<dyn crate::command::CommandInterface>,
        descriptor: crate::command::CommandDescriptor,
    ) {
        self.extension
            .register_command_with_descriptor::<C>(descriptor);
    }

    /// Registers a command implementation with runtime metadata.
    pub fn register_command_with_descriptor<C: CommandExtension>(
        &mut self,
        descriptor: crate::command::CommandDescriptor,
    ) {
        self.register_extension_command::<C>(crate::command::COMMANDS_POINT, descriptor);
    }
}

impl Default for ExtensionBuilder {
    fn default() -> Self {
        Self::new()
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
