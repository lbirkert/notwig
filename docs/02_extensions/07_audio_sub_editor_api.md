# Audio Sub-editor API

Status: implemented for shared curve sub-editors. Extensions can add an entry to
the audio editor's sub-editor selector. Curve sub-editors use the host's global
curve editor and the canonical clip data; the extension does not draw or store
the curve.

The reference implementation is the clip pan editor in
`crates/notwig-ext/notwig-ext-example`.

## Manifest

Audio sub-editor contributions require `manifest_version: 2` and the
`audio_editor` capability.

```json
{
  "manifest_version": 2,
  "id": "com.example.clip-tools",
  "name": "Clip Tools",
  "vendor": "Example",
  "version": "0.1.0",
  "api_version": 5,
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
        "curve": {
          "target": "pan",
          "value_min": -1.0,
          "value_max": 1.0,
          "baseline": 0.0,
          "format": "pan"
        }
      }
    }
  ]
}
```

The selector lists built-in `Audio Events` and `Gain` first, then extension
entries by category, `order`, title, and ID. Disabling or uninstalling the
extension removes its entry. If it was active, the audio editor falls back to
Audio Events.

`order` is an integer; lower values appear first inside a category. `title` is
limited to 64 bytes.

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

### Why the extension cannot store the curve

A curve that lives only inside extension state cannot be saved with the project
or affect playback. The host deliberately exposes only host-modeled curve
targets. A future extension-defined target needs a canonical data contract,
engine path, and persistence before it can be contributed.

## Custom sub-editors

Custom, extension-drawn sub-editors are not part of this contract yet. A future
custom editor must use the bounded panel surface and return host-owned edit
families; it cannot receive the project document or mutate canonical state
directly.
