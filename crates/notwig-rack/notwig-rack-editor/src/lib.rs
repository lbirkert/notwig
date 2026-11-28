//! Native editor-window hosting for [`notwig_rack_core`] plugins.
//!
//! Format wrappers implement [`notwig_rack_core::PluginEditor`]; this crate
//! owns the outer native window and the per-frame lifecycle an editor needs.
//! It is format-agnostic: the caller supplies a loaded
//! [`notwig_rack_core::Plugin<f32>`], a title, a sample rate, and a maximum
//! block size. The window then:
//!
//! - activates the plugin with a stereo layout,
//! - finds and embeds the plugin editor through
//!   [`notwig_rack_core::PluginCore::editor`],
//! - pumps the plugin idle callback and keeps the outer window sized to the
//!   editor's reported size,
//! - maps raw parameter ids to plugin indices for host-to-plugin and
//!   plugin-to-host parameter changes,
//! - saves and restores plugin state blobs,
//! - drains transport-toggle requests produced while a plugin window is key,
//! - renders a stereo monitor block through the editor's own plugin instance,
//! - captures the window contents as PNG for tests and diagnostics.
//!
//! Everything runs on the main/UI thread. Audio processing for the editor
//! instance is explicitly separate from the engine's DSP instance; callers
//! bridge monitor audio into [`EditorWindow::process_interleaved`].
//!
//! # Platform support
//!
//! Native windows are currently implemented with `AppKit` on macOS. Other
//! targets get a stub whose [`EditorWindow::open`] returns an error and whose
//! pump functions are no-ops, so callers can stay platform-independent.

#![allow(unsafe_code)]

mod window;

#[cfg(target_os = "macos")]
mod appkit;
#[cfg(target_os = "macos")]
mod space_monitor;

pub use window::EditorWindow;

#[cfg(target_os = "macos")]
pub use appkit::{prepare_window_host, pump_window_host};
#[cfg(not(target_os = "macos"))]
pub use window::{prepare_window_host, pump_window_host};
