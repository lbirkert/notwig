# Audio Sub-editor API

Status: implemented for extension-rendered audio sub-editors. Extensions add an
entry to the audio editor's sub-editor selector, then register an editor with
runtime metadata. The host owns the surrounding editor, clip context,
validation, revision check, and undo boundary; the extension draws the panel.

The reference implementation is the clip pan editor in
`crates/notwig-ext/notwig-ext-example`.

## Runtime metadata

Audio sub-editors require `manifest_version: 2` and the `audio_editor`
capability. The descriptor is returned by `notwig_ext_registrations` after the
load phase:

```json
{
  "manifest_version": 2,
  "id": "com.example.clip-tools",
  "name": "Clip Tools",
  "vendor": "Example",
  "version": "0.1.0",
  "api_version": 8,
  "capabilities": ["audio_editor"]
}
```

There is no `kind` field. The selector lists built-in `Audio Events` and `Gain`
first, then extension entries by category, `order`, title, and ID. Disabling or
uninstalling the extension removes its entry. If it was active, the audio editor
falls back to Audio Events.

`order` is an integer; lower values appear first inside a category. `title` is
limited to 64 bytes.

## Registering the renderer

Implement `AudioSubEditorExtension` and register it with runtime metadata:

```rust
use notwig_ext_api::{
    AUDIO_SUB_EDITOR_EXTENSION_POINT, AudioSubEditorContext, AudioSubEditorDescriptor,
    AudioSubEditorExtension, CommandEdit, CommandEffect, ExtensionBuilder, notwig_ext,
};

struct PanEditor;

impl AudioSubEditorExtension for PanEditor {
    fn new() -> Self { Self }

    fn draw(
        &mut self,
        ui: &mut notwig_ext_api::egui::Ui,
        context: &AudioSubEditorContext,
    ) -> notwig_ext_api::AudioSubEditorResponse {
        let _ = ui;
        let _ = context;
        notwig_ext_api::AudioSubEditorResponse::none()
    }
}

#[notwig_ext(load)]
pub fn load(extension: &mut ExtensionBuilder) {
    extension.register_extension_audio_sub_editor::<PanEditor>(
        AUDIO_SUB_EDITOR_EXTENSION_POINT,
        AudioSubEditorDescriptor {
            id: "com.example.clip-tools.pan".into(),
            title: "Clip Pan".into(),
            category: "Audio".into(),
            order: 10,
            extension_id: None,
        },
    );
}
```

One guest instance is retained per clip and registration id so switching clips
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
- resolved selection gestures (`copy`, `cut`, `paste`, `duplicate`, `delete`,
  `select_all`, and clip-local `paste_at`) and the shared curve clipboard;
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

Selection operations are shared with the built-in clip editors. Call
`notwig_ui::extension_ui::apply_curve_edit_actions` with `context.actions` and
`context.clipboard` before drawing. It implements the same select-all, copy,
cut, paste, duplicate, and delete semantics as the Gain editor. Publish copied
or cut points through `AudioSubEditorResponse::clipboard`; the host updates the
shared curve clipboard so pan and Gain can paste between each other.

## Edits and undo

The guest returns an `AudioSubEditorResponse` containing a `CommandEffect` and
optional clipboard points. The host accepts the same host-owned edit families as
commands, including `AudioSlices`, `AudioCurves`, `ClipFields`, and UI requests. `AudioCurves` carries an optional gain change and an optional pan
change; the outer option means "leave unchanged" and the inner option sets or
clears the curve.

Return `CommandEffect::None` for frames that do not commit an edit. The shared
curve editor reports `committed` when a gesture ends; return an `Apply` effect
only then. The host applies it as one undo batch after checking that the context
revision is still current. A stale effect is rejected with a notification.

Custom sub-editors cannot receive the project document, mutate canonical state
directly, draw outside their panel, or store playback-affecting data in hidden
extension state. Use a host edit family, a device, or a host render job.
