//! Replaceable clipboard extension point.

use notwig_core::Clipboard;

use crate::extension_point::ExtensionPoint;

/// Host-side clipboard provider contract.
///
/// Exactly one provider is active. Core provides the default implementation;
/// an extension can replace it through the clipboard point.
pub trait ClipboardProvider: Send + Sync {
    /// Returns the current structured clipboard payload.
    fn get(&self) -> Clipboard;

    /// Replaces the structured clipboard payload.
    fn set(&self, clipboard: Clipboard);

    /// Returns the platform-text payload, when one is cached.
    fn get_text(&self) -> Option<String>;

    /// Replaces the platform-text payload.
    fn set_text(&self, text: Option<String>);
}

/// Single-provider extension point for the shared clipboard.
pub const EP_NOTWIG_CLIPBOARD: ExtensionPoint<dyn ClipboardProvider> =
    ExtensionPoint::single("notwig.clipboard");
