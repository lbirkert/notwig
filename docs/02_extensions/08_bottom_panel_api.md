# Bottom Panel API

Status: implemented. Extensions can contribute arbitrary editor bottom panels
through the single `EP_NOTWIG_BOTTOM_PANEL` / `notwig.bottom_panel` extension
point. The shell keeps at most one bottom panel open at a time.

## Registration

Implement `BottomPanelExtension` and register it with the shared
`register_extension` method:

```rust
use notwig_ext_api::{
    BottomPanelContext, BottomPanelDescriptor, BottomPanelExtension, BottomPanelResponse,
    EP_NOTWIG_BOTTOM_PANEL,
};

pub const PANEL_ID: &str = "com.example.notes";

struct NotesPanel;

impl BottomPanelExtension for NotesPanel {
    fn new() -> Self {
        Self
    }

    fn descriptor() -> BottomPanelDescriptor {
        BottomPanelDescriptor {
            id: PANEL_ID.into(),
            title: "Notes".into(),
            order: 50,
            extension_id: None,
        }
    }

    fn draw(
        &mut self,
        ui: &mut notwig_ext_api::egui::Ui,
        context: &BottomPanelContext,
    ) -> BottomPanelResponse {
        ui.heading("Notes");
        ui.label(format!("Revision {}", context.revision));
        BottomPanelResponse::none()
    }
}
```

```rust
#[notwig_ext(load)]
pub fn load(extension: &mut ExtensionBuilder) {
    extension.register_extension::<NotesPanel>(EP_NOTWIG_BOTTOM_PANEL);
}
```

The manifest must include the `bottom_panel` capability. The host reads the
panel id and title from the post-load registration manifest.

## Opening a panel

Commands can open a registered panel with
`CommandUiRequest::OpenBottomPanel`:

```rust
fn execute(&mut self, _context: &CommandContext) -> CommandEffect {
    CommandEffect::Apply {
        edits: Vec::new(),
        ui: vec![CommandUiRequest::OpenBottomPanel {
            id: PANEL_ID.into(),
        }],
    }
}
```

Opening a package panel closes the active built-in editor panel. Opening a
built-in editor closes the active package panel.

## Context

`BottomPanelContext` is a bounded snapshot containing the panel id, document
revision, panel input, and theme. The guest draws with its private egui context
and returns a validated `PanelFrame`; host actions are validated before use.

Built-in panels use the same extension point and are owned by the fixed,
non-uninstallable `notwig.core` / `Notwig Core` extension.
