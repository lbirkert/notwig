//! Format-agnostic ARA factory and binding handles.
//!
//! Companion crates (`notwig-rack-clap`, `notwig-rack-vst3`, and
//! `notwig-rack-au`) query their native ARA entry point and construct these
//! handles. The handles borrow nothing at the Rust level, but the raw pointers
//! remain valid only while the plugin instance that produced them is alive.
//! [`super::AraDocument`] documents that lifetime contract.

use ara2_bridge_core::AraError;
use ara2_bridge_sys::{ARADocumentControllerRef, ARAFactory, ARAPlugInExtensionInstance};
use std::ptr::NonNull;

/// One ARA factory exposed by a loaded plugin.
///
/// Obtain this from a format crate through [`AraPlugin::ara_factory`]. The
/// handle is a thin wrapper around the plugin-owned `ARAFactory` pointer; it
/// does not initialize the factory. Initialization happens when the host
/// builds an [`super::AraDocument`].
#[derive(Clone, Copy)]
pub struct AraFactory {
    raw: NonNull<ARAFactory>,
}

impl AraFactory {
    /// Creates a handle from the raw factory pointer reported by a plugin.
    ///
    /// # Safety
    ///
    /// `raw` must point at a live `ARAFactory` whose backing remains readable
    /// for at least as long as the plugin instance and any document built from
    /// it. The caller must not initialize the same factory through another
    /// `LoadedFactory` concurrently.
    ///
    /// # Errors
    ///
    /// Returns [`AraError::InvalidArgument`] when `raw` is null.
    pub unsafe fn from_raw(raw: *const ARAFactory) -> Result<Self, AraError> {
        let raw =
            NonNull::new(raw.cast_mut()).ok_or(AraError::InvalidArgument("null ARA factory"))?;
        Ok(Self { raw })
    }

    /// Returns the raw factory pointer.
    #[must_use]
    pub fn as_raw(self) -> *const ARAFactory {
        self.raw.as_ptr()
    }
}

impl std::fmt::Debug for AraFactory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AraFactory")
            .field("raw", &self.raw.as_ptr())
            .finish()
    }
}

/// One companion-owned ARA plug-in extension instance returned by binding.
///
/// The extension is owned by the plugin. The host may attach it to a document
/// through [`super::AraDocument::bind`]; the document keeps the returned
/// controller alive until it is dropped.
#[derive(Clone, Copy)]
pub struct AraPluginExtension {
    raw: NonNull<ARAPlugInExtensionInstance>,
}

impl AraPluginExtension {
    /// Creates a handle from a companion binding result.
    ///
    /// # Safety
    ///
    /// `raw` must be the non-null instance returned by the same plugin instance
    /// after binding it to the document controller supplied by the host. It
    /// must remain valid until the plugin and document are both destroyed.
    ///
    /// # Errors
    ///
    /// Returns [`AraError::Peer`] when `raw` is null.
    pub unsafe fn from_raw(raw: *const ARAPlugInExtensionInstance) -> Result<Self, AraError> {
        let raw = NonNull::new(raw.cast_mut())
            .ok_or(AraError::Peer("companion returned a null ARA extension"))?;
        Ok(Self { raw })
    }

    /// Returns the raw extension pointer.
    #[must_use]
    pub fn as_raw(self) -> *const ARAPlugInExtensionInstance {
        self.raw.as_ptr()
    }
}

impl std::fmt::Debug for AraPluginExtension {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AraPluginExtension")
            .field("raw", &self.raw.as_ptr())
            .finish()
    }
}

/// A loaded plugin that can expose ARA through its companion format.
///
/// Implemented by `notwig-rack-clap`, `notwig-rack-vst3`, and
/// `notwig-rack-au`. The object must remain alive for the full lifetime of any
/// [`super::AraDocument`] built from its factory and for any extension binding
/// attached to that document.
pub trait AraPlugin {
    /// Returns the plugin's ARA factory, or `None` for non-ARA plugins.
    fn ara_factory(&self) -> Option<AraFactory>;

    /// Binds this plugin instance to the supplied document controller.
    ///
    /// Implementations call the companion API's role-aware binding entry point.
    /// This must happen before activation, state load, processing-related
    /// extension use, or editor creation, and it may be called only once per
    /// instance.
    ///
    /// # Safety
    ///
    /// `controller` must be a live document-controller reference owned by the
    /// caller's [`super::AraDocument`], which must outlive the binding.
    ///
    /// # Errors
    ///
    /// Returns the companion or ARA error when discovery, binding, or role
    /// validation fails.
    unsafe fn bind_ara(
        &mut self,
        controller: ARADocumentControllerRef,
        known: super::AraRoles,
        assigned: super::AraRoles,
    ) -> Result<AraPluginExtension, AraError>;

    /// Returns `true` when [`AraPlugin::ara_factory`] yields a factory.
    fn has_ara(&self) -> bool {
        self.ara_factory().is_some()
    }
}
