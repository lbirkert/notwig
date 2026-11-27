//! Extension-defined editor bottom panels.

use crate::egui;
use serde::{Deserialize, Serialize};

use crate::command::{CommandEffect, MAX_COMMAND_EDITS};
use crate::context::ThemeSnapshot;
use crate::extension_point::ExtensionPoint;
use crate::panel_frame::{PanelFrame, PanelInput};

/// Marker interface for bottom-panel implementations.
pub trait BottomPanelInterface {}

/// Extension point for editor bottom panels.
///
/// The shell keeps at most one bottom panel open at a time, selected by a
/// stable panel id.
pub const EP_NOTWIG_BOTTOM_PANEL: ExtensionPoint<dyn BottomPanelInterface> =
    ExtensionPoint::new("notwig.bottom_panel");

/// Maximum length of a bottom-panel title.
pub const MAX_BOTTOM_PANEL_TITLE_BYTES: usize = 64;

/// Resolved metadata for one extension bottom panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BottomPanelDescriptor {
    /// Stable panel id used by commands and host state.
    pub id: String,
    /// Panel title shown in menus and headers.
    pub title: String,
    /// Ordering hint for panel pickers.
    #[serde(default)]
    pub order: i32,
    /// Owning extension id.
    #[serde(default)]
    pub extension_id: Option<String>,
}

/// Bounded state supplied to an extension-drawn bottom panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BottomPanelContext {
    /// Stable panel id.
    pub panel_id: String,
    /// Document revision at snapshot time.
    pub revision: u64,
    /// Panel input snapshot.
    pub input: PanelInput,
    /// Host theme snapshot.
    pub theme: ThemeSnapshot,
}

/// Result returned by one bottom-panel draw.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BottomPanelResponse {
    /// Host-owned edits requested by this frame.
    pub effect: CommandEffect,
}

impl Default for BottomPanelResponse {
    fn default() -> Self {
        Self {
            effect: CommandEffect::None,
        }
    }
}

impl BottomPanelResponse {
    /// Returns a response with no edits.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }
}

/// Serialized frame returned by an extension bottom panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BottomPanelFrame {
    /// Tessellated guest UI.
    pub panel: PanelFrame,
    /// Host-owned edits requested by this frame.
    pub effect: CommandEffect,
}

impl BottomPanelFrame {
    /// Validates the panel payload and edit budget.
    pub fn validate(&self) -> Result<(), String> {
        self.panel
            .validate()
            .map_err(|error| format!("invalid bottom-panel frame: {error}"))?;
        if let CommandEffect::Apply { edits, .. } = &self.effect {
            if edits.len() > MAX_COMMAND_EDITS {
                return Err("bottom panel returned too many edits".into());
            }
        }
        if let CommandEffect::Error { message } = &self.effect {
            if message.len() > 4096 {
                return Err("bottom-panel error message is too long".into());
            }
        }
        Ok(())
    }
}

/// Author-facing extension-drawn bottom panel.
pub trait BottomPanelExtension: Send + Sized + 'static {
    /// Creates one bottom-panel instance.
    fn new() -> Self;

    /// Returns the panel metadata used by the host shell.
    fn descriptor() -> BottomPanelDescriptor;

    /// Draws the panel and returns requested host edits.
    fn draw(&mut self, ui: &mut egui::Ui, context: &BottomPanelContext) -> BottomPanelResponse;
}

/// Host-side drawer for extension bottom panels.
pub trait BottomPanelHost {
    /// Draws the panel identified by `panel_id`.
    fn draw_bottom_panel(
        &mut self,
        panel_id: &str,
        context: &BottomPanelContext,
    ) -> Option<BottomPanelFrame>;
}
