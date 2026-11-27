# Audio Sub-editor API

Status: implemented for shared curve sub-editors and extension-drawn custom
sub-editors. Extensions can add an entry to the audio editor's sub-editor
selector. A contribution either binds a host-owned curve to the shared editor or
draws its own panel over a bounded audio-editor context.

The reference implementation is the clip pan editor in
`crates/notwig-ext/notwig-ext-example`.

## Manifest

Audio sub-editor contributions require `manifest_version: 2` and the
`audio_editor` capability. The `kind` field selects one of two behaviors.

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
      "order": 10,
      "kind": {
        "custom": {
          "renderer": "com.example.clip-tools.pan_editor"
        }
      }
    }
  ]
}
```

A shared curve sub-editor uses a host-owned curve instead:

```json
"kind": {
  "curve": {
    "target": "pan",
    "value_min": -1.0,
    "value_max": 1.0,
    "baseline": 0.0,
    "format": "pan"
  }
}
```

The selector lists built-in `Audio Events` and `Gain` first, then extension
entries by category, `order`, title, and ID. Disabling or uninstalling the
extension removes its entry. If it was active, the audio editor falls back to
Audio Events.

`order` is an integer; lower values appear first inside a category. `title` is
limited to 64 bytes. A custom `renderer` id is limited to 128 bytes and must be
registered by the extension at runtime.

## Curve sub-editors

A curve contribution declares a host-owned target and how to present it:

| Field | Meaning |
| --- | --- |
| `target` | Canonical clip curve: `gain` or `pan`. |
| `value_min`, `value_max` | Visible value range in the shared curve editor. |
| `baseline` | Optional horizontal guide, such as zero dB or center pan. |
| `format` | Hover formatting: `decibels` or `pan`. |

The host owns the global curve editor, time selection, snapping, viewport,
preview, commit, and undo grouping. The extension only supplies this declaration.

The value range must be finite and increasing. The baseline, when present, must
lie inside the range.

### Canonical data

`gain` edits `AudioClipData::gain_curve`; `pan` edits
`AudioClipData::pan_curve`. Both are time-varying `notwig_core::Curve` values:

- Gain is an offset in dB on top of clip and event gain.
- Pan is `-1..=1`: `-1` is hard left, `0` is center, and `1` is hard right. A
  missing pan curve leaves the clip unchanged; a flat zero curve is also unity
  gain because clip pan uses a balance law rather than center attenuation.

Curve points follow the same rules as the Gain sub-editor. They may sit before
or after the clip range; playback holds the nearest endpoint. Clip copy,
duplicate, extract, and trim carry the curve and shift its local times.

Edits are previewed through the engine while dragging and committed as one
undo entry on pointer release. Selection, copy, cut, paste, duplicate, delete,
select-all, fine adjustment, and segment curvature use the same audio-editor
actions and clipboard format as the Gain sub-editor.

## Custom sub-editors

A custom contribution names a renderer id registered by the extension. The
guest draws its own panel with the real egui API inside its private context; the
host validates the returned `PanelFrame` and composites it into the audio-editor
content area.

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
    extension.register_audio_sub_editor::<PanEditor>("com.example.clip-tools.pan_editor");
}
```

The renderer id in `register_audio_sub_editor` must match
`kind.custom.renderer`. One guest instance is retained per clip and
contribution id so switching clips or sub-editors preserves view state. The
host destroys it when the extension reloads or the extension host is dropped.

### Context

`AudioSubEditorContext` is a bounded snapshot, not the project document:

- `editor_id`, `revision`, `clip_id`, `track_id`, and `clip_range`;
- `AudioSubEditorClip`, a postcard-safe snapshot of the canonical clip data,
  including slices and host-owned gain/pan curves;
- selected slices and optional clip-local time selection;
- playhead, loop range, tempo, sample rate, snap state, grid step, viewport, and
  zoom;
- panel size, theme, and the same pointer/keyboard input snapshot used by device
  panels.

The guest must not retain the context or any reference derived from it. Cache
view state on the sub-editor instance.

### Edits and undo

The guest returns a `CommandEffect`. The host accepts the same host-owned edit
families as commands, including `AudioSlices`, `AudioCurves`, `ClipFields`, and
UI requests. `AudioCurves` carries an optional gain change and an optional pan
change; the outer option means "leave unchanged" and the inner option sets or
clears the curve.

Return `CommandEffect::None` for frames that do not commit an edit. The host
applies an `Apply` effect as one undo batch, after checking that the context
revision is still current. A stale effect is rejected with a notification.

Custom sub-editors cannot receive the project document, mutate canonical state
directly, draw outside their panel, or store playback-affecting data in hidden
extension state. Use a host edit family, a device, or a host render job.

### Why the extension cannot store the curve

A curve that lives only inside extension state cannot be saved with the project
or affect playback. The host deliberately exposes only host-modeled curve
targets. A future extension-defined target needs a canonical data contract,
engine path, and persistence before it can be contributed.
