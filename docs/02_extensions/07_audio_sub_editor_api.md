# Audio Sub-editor API

Status: implemented for extension-rendered audio sub-editors. Extensions add an
entry to the audio editor's sub-editor selector, then register an editor under
the contribution id. The host owns the surrounding editor, clip context,
validation, revision check, and undo boundary; the extension draws the panel.

The reference implementation is the clip pan editor in
`crates/notwig-ext/notwig-ext-example`.

## Manifest

Audio sub-editor contributions require `manifest_version: 2` and the
`audio_editor` capability. The contribution id is also the renderer id the
extension registers at runtime.

```json
{
  "manifest_version": 2,
  "id": "com.example.clip-tools",
  "name": "Clip Tools",
  "vendor": "Example",
  "version": "0.1.0",
  "api_version": 6,
  "capabilities": ["audio_editor"],
  "contributes": [
    {
      "id": "com.example.clip-tools.pan",
      "point": "notwig.audio_editor.sub_editor",
      "contract": "notwig.audio_editor.sub_editor@1",
      "title": "Clip Pan",
      "category": "Audio",
      "order": 10
    }
  ]
}
```

There is no `kind` field. The selector lists built-in `Audio Events` and `Gain`
first, then extension entries by category, `order`, title, and ID. Disabling or
uninstalling the extension removes its entry. If it was active, the audio editor
falls back to Audio Events.

`order` is an integer; lower values appear first inside a category. `title` is
limited to 64 bytes.

## Registering the renderer

Implement `AudioSubEditorExtension` and register it under the contribution id:

```rust
use notwig_ext_api::{
    AudioSubEditorContext, AudioSubEditorExtension, CommandEdit, CommandEffect, Extension,
    notwig_extension,
};

struct PanEditor;

impl AudioSubEditorExtension for PanEditor {
    fn new() -> Self { Self }

    fn draw(
        &mut self,
        ui: &mut notwig_ext_api::egui::Ui,
        context: &AudioSubEditorContext,
    ) -> CommandEffect {
        let _ = ui;
        let _ = context;
        CommandEffect::None
    }
}

#[notwig_extension]
pub fn extension(extension: &mut Extension) {
    // Must match the contribution id in extension.json.
    extension.register_audio_sub_editor::<PanEditor>("com.example.clip-tools.pan");
}
```

One guest instance is retained per clip and contribution id so switching clips
or sub-editors preserves view state. The host destroys it when the extension
reloads or the extension host is dropped.

## Context

`AudioSubEditorContext` is a bounded snapshot, not the project document:

- `editor_id`, `revision`, `clip_id`, `track_id`, and `clip_range`;
- `AudioSubEditorClip`, a postcard-safe snapshot of the canonical clip data,
  including slices and host-owned gain/pan curves;
- selected slices and optional clip-local time selection;
- playhead, loop range, tempo, sample rate, snap state, grid, bar/beat lengths,
  viewport, and zoom;
- `AudioWaveformPreview`, a host-rendered waveform underlay with one min/max
  column per panel point;
- panel size, theme, and the same pointer/keyboard input snapshot used by device
  panels.

The guest must not retain the context or any reference derived from it. Cache
view state on the sub-editor instance.

## Shared curve editor

`notwig_ui` exposes an extension-facing curve editor when the `extension-ui`
feature is enabled:

```toml
notwig-ui = { workspace = true, default-features = false, features = ["extension-ui"] }
```

Use `notwig_ui::extension_ui::show_with_background` to draw the shared point,
segment, marquee, snapping, and fine-adjustment editor. Draw
`AudioWaveformPreview` through `notwig_ui::extension_ui::draw_waveform_preview`
in the background closure. The pan example uses both so it looks and behaves
like the host Gain editor.

The context already contains the timeline mapping fields needed to construct a
`TimelineGrid`. The editor instance should retain `LineEditorState`, selection,
and time-selection state across frames.

## Edits and undo

The guest returns a `CommandEffect`. The host accepts the same host-owned edit
families as commands, including `AudioSlices`, `AudioCurves`, `ClipFields`, and
UI requests. `AudioCurves` carries an optional gain change and an optional pan
change; the outer option means "leave unchanged" and the inner option sets or
clears the curve.

Return `CommandEffect::None` for frames that do not commit an edit. The shared
curve editor reports `committed` when a gesture ends; return an `Apply` effect
only then. The host applies it as one undo batch after checking that the context
revision is still current. A stale effect is rejected with a notification.

Custom sub-editors cannot receive the project document, mutate canonical state
directly, draw outside their panel, or store playback-affecting data in hidden
extension state. Use a host edit family, a device, or a host render job.
