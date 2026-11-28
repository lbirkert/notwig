//! Example extension-defined bottom panel and the command that opens it.

use notwig_ext_api::{
    BottomPanelContext, BottomPanelDescriptor, BottomPanelExtension, BottomPanelResponse,
    CommandContext, CommandDescriptor, CommandEffect, CommandExtension, CommandUiRequest,
};

/// Stable id for the example bottom panel.
pub const EXAMPLE_PANEL_ID: &str = "notwig.ext.example.notes";
/// Stable command id for opening the example bottom panel.
pub const OPEN_EXAMPLE_PANEL_COMMAND_ID: &str = "notwig.ext.example.open_panel";

/// Extension-drawn bottom panel.
#[derive(Default, notwig_ext_api::ExtensionCall)]
#[extension_call(BottomPanelExtension)]
pub struct ExampleBottomPanel {
    frames: u64,
}

impl BottomPanelExtension for ExampleBottomPanel {
    fn new() -> Self {
        Self::default()
    }

    fn descriptor() -> BottomPanelDescriptor {
        BottomPanelDescriptor {
            id: EXAMPLE_PANEL_ID.into(),
            title: "Example Notes".into(),
            order: 50,
            extension_id: None,
        }
    }

    fn draw(
        &mut self,
        ui: &mut notwig_ext_api::egui::Ui,
        context: &BottomPanelContext,
    ) -> BottomPanelResponse {
        self.frames = self.frames.saturating_add(1);
        ui.heading("Example Bottom Panel");
        ui.label(format!(
            "Revision {} · frame {}",
            context.revision, self.frames
        ));
        ui.label("Registered through EP_NOTWIG_BOTTOM_PANEL.");
        BottomPanelResponse::none()
    }
}

/// Command that opens the example bottom panel.
#[derive(Default, notwig_ext_api::ExtensionCall)]
#[extension_call(CommandExtension)]
pub struct OpenExamplePanelCommand;

impl CommandExtension for OpenExamplePanelCommand {
    fn new() -> Self {
        Self
    }

    fn descriptor() -> CommandDescriptor {
        CommandDescriptor {
            id: OPEN_EXAMPLE_PANEL_COMMAND_ID.into(),
            title: "Open Example Bottom Panel".into(),
            category: "View".into(),
            description: "Open the bottom panel contributed by the example extension.".into(),
            when: None,
            default_bindings: Vec::new(),
            extension_id: None,
        }
    }

    fn execute(&mut self, _context: &CommandContext) -> CommandEffect {
        CommandEffect::Apply {
            edits: Vec::new(),
            ui: vec![CommandUiRequest::OpenBottomPanel {
                id: EXAMPLE_PANEL_ID.into(),
            }],
        }
    }
}
