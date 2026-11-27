//! CLAP ARA companion extension.
//!
//! `ARACLAP.h` adds one plug-in extension that exposes the ARA factory and the
//! role-aware binding call. The host does not need a matching host extension.

use clap_sys::plugin::clap_plugin;
use notwig_rack_ara::{
    ARADocumentControllerRef, ARAFactory, ARAPlugInExtensionInstance, ARAPlugInInstanceRoleFlags,
};
use std::ffi::CStr;

/// Current ARA CLAP plug-in extension id.
pub(crate) static CLAP_EXT_ARA_PLUGINEXTENSION: &CStr = c"org.ara-audio.ara.pluginextension/2";

/// Draft-compatible ARA CLAP plug-in extension id.
pub(crate) static CLAP_EXT_ARA_PLUGINEXTENSION_COMPAT: &CStr =
    c"org.ara-audio.ara.pluginextension.draft/2";

/// `clap_ara_plugin_extension_t` from `ARACLAP.h`.
#[repr(C)]
pub(crate) struct ClapAraPluginExtension {
    /// Returns the ARA factory associated with this plug-in instance.
    pub(crate) get_factory:
        Option<unsafe extern "C" fn(plugin: *const clap_plugin) -> *const ARAFactory>,
    /// Binds the instance to a document controller with explicit roles.
    pub(crate) bind_to_document_controller: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin,
            document_controller: ARADocumentControllerRef,
            known_roles: ARAPlugInInstanceRoleFlags,
            assigned_roles: ARAPlugInInstanceRoleFlags,
        ) -> *const ARAPlugInExtensionInstance,
    >,
}
