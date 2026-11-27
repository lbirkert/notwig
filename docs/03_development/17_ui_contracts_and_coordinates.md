# UI contracts and coordinates

Status: implemented. This page records the boundary between the UI and the
document, the focus model, panel sizing, and the time/sample/pixel mappings
used by the arranger and clip editors.

Related: [`Editing`](03_editing.md), [`Waveform Rendering`](10_waveform_rendering.md),
[`Extension Panels`](12_extension_panels.md), and
[`crates/notwig-ui/src`](../../crates/notwig-ui/src).

## 1. View and intent flow

`ProjectDoc` is canonical and read-only to views. `RuntimeView` is a per-frame
projection of playhead, meters, waveforms, plugin/extension info, and audio
state. `NotwigUi::show(&ProjectDoc, &RuntimeView)` returns a `Vec<UiIntent>`;
the app consumes that vector after the frame and applies each intent through
`Workspace` or a dialog handler. The UI never calls a document command
directly.

```text
ProjectDoc ──────────────┐ (read)
                         v
RuntimeView ───────► NotwigUi::show ─────► Vec<UiIntent>
                         │                        │
                         │ mutates only           │ after the frame
                         v                        v
                    UiState + view state     app handle_intent
                    (selection, viewport,          │
                     drags, dialogs, caches)       v
                                             Workspace commands
                                                   │
                                                   v
                                             ProjectDoc + undo
```

`UiIntent` is one enum owned by `notwig-ui`; `emit` deduplicates an identical
intent within one frame. Its families and owners are:

| Family | Examples | Applied by |
| --- | --- | --- |
| Transport | `Play`, `Stop`, `SetPlayhead`, `SetLoopRange` | `Workspace`/transport |
| Project and export | `Save`, `SaveAs`, `RenderMixdownRange` | app dialogs/workspace |
| Selection notices | `SelectTrack`, `SelectClip`, `SelectDevice` | ignored; UI already updated selection |
| Arranger edits | `MoveClips`, `PasteClips`, `DeleteSelection`, `SetClipSlices` | workspace/command lowering |
| Track/mixer | `CreateTrack`, `GroupTracks`, `SetTrackMixer` | workspace |
| MIDI | `CreateMidiNote`, `EditMidiNotes`, `PreviewNote` | workspace/engine |
| Devices | `InsertDevice`, `RemoveDevice`, `SetParam` | workspace/plugin host |
| Extensions/settings | `InstallExtensionZip`, `SetExtensionEnabled`, `ApplyAudioSettings` | app |

The app ignores selection notices because selection is UI state.

## 2. What views may mutate

Views may mutate ephemeral state only: `UiState` (selection, viewport, snap,
notifications, peak hold), editor state (`ClipEditorState`,
`MidiEditorState`), drag/preview origins, collapse sets, dialogs, pointer/press
origins, content rectangles, and extension caches. They may not insert, remove,
rename, trim, or reorder anything in `ProjectDoc`.

Color is presentation state stored on the entity as `Option<Color>`, where
`None` means inherit. The UI resolves a clip's inherited color from its track,
then the track's parent groups, then the track-kind fallback. The UI never
writes an inherited color back into the document; choosing the `×` swatch emits
a color intent with `None`. Clip name and color changes use the existing
`EditClip`/`SetClipColor` commands through `UiIntent::RenameClip` and
`UiIntent::SetClipColor`, so they participate in the normal undo history.

## 3. Selection, focus, and editor exclusivity

Selection is a group of tracks, clips, devices, and a time selection. Each
`SelectionGroup` is an ordered `BTreeSet` plus a primary item. A time selection
is a `RangeMus` plus its lanes. `Selection::to_edit_selection` projects this to
the core edit selection. The MIDI editor keeps its note selection and note
clipboard inside `MidiEditorState` rather than the global `Selection.notes`.
Selection changes are mode-aware: a non-additive object selection clears the
time selection, object mode has priority over time mode, and stale IDs are
retained until materialization.

`OpenedEditor` is exactly one of `DeviceEditor`, `ClipEditor`, `MidiEditor`, or
none, so the panels are mutually exclusive. `EditFocus` is `Timeline`,
`ClipEditor`, or `MidiEditor`. Opening a clip/MIDI editor focuses it; opening
the device editor or clicking the arranger returns focus to the timeline.
There is no independent keyboard focus per panel.

## 4. Keyboard routing and the one-frame lag

`handle_shortcuts` runs at the start of `show`, before panels and the arranger
are drawn. It returns early when the shortcut editor is capturing a chord, when
an extension panel requested keyboard input, or when `egui` already wants
keyboard input. Copy/cut/paste, select-all, join, duplicate, delete, and cancel
are gated on `editor_focused`, which requires `edit_focus`, the matching
`OpenedEditor`, and a live editor state. Transport, save, undo, track creation,
and extension/application shortcuts remain global.

A pointer press that changes focus is processed later in the same frame while
drawing the arranger or an editor, and `extension_panel_keyboard` is set while
drawing the device panel. Both are read by the next frame's shortcut pass, so
focus changes and panel shortcut suppression take effect one frame later.

## 5. Panel hosting and persisted heights

Bottom panels use `egui::TopBottomPanel` with a stable `Id` and a
`PanelConfig`. A resizable panel stores its height in egui; the host calls
`ui.set_min_size(ui.available_size())` so short content does not shrink it.
Dragging below `collapse_height` closes the panel, after which the saved
`PanelState` is reset to the default height so reopening does not immediately
collapse. The device panel is non-resizable and uses an exact height. The info
panel is a fixed-width left side panel.

| Panel | Kind | Default | Bounds |
| --- | --- | --- | --- |
| Device editor | bottom, fixed | 260 px | exact 260 px |
| Audio clip editor | bottom, resizable | 420 px | 120–700 px, collapses below 180 |
| MIDI editor | bottom, resizable | 440 px | 160–800 px, collapses below 220 |
| Info | left, fixed | 250 px wide | exact width |

## 6. Coordinate systems

`TimeMus` is an exact rational in quarter-note beats. UI arithmetic usually
constructs it as a 1/1,000,000-beat tick ("microbeat") to keep hit tests and
document times comparable. `TimeAudio(u64)` exists, but current audio windows
use raw `u64` sample indices; sample time enters through the tempo map:
`samples_per_beat = sample_rate * 60 / tempo_bpm`, `mus_to_samples` rounds and
clamps non-positive values to zero, and `samples_to_mus` rounds to a microbeat.

| Coordinate | Unit and origin | Conversion |
| --- | --- | --- |
| Arranger time | absolute quarter-note beats from project zero | none; clip ranges and time selections are absolute |
| Clip-local event time | beats from `clip.range.start` | `absolute = clip_start + local` |
| MIDI content time | beats from `midi.content_start` | `content = content_start + (beat - clip_start)`; `beat = clip_start + (content - content_start)` |
| Audio source window | sample indices into the asset buffer | `AudioClipData`/`AudioSlice` `source_start_sample..source_end_sample`; no stretching |
| Silent prefix | samples before source audio inside a slice | `duration = silent_prefix + (source_end - source_start)` |
| Pixels | screen x relative to the timeline content left | `x = left + (beat - viewport.start) * pixels_per_beat` |
| Beat from x | inverse of the pixel map | `beat = viewport.start + (x - left) / pixels_per_beat` |

`Viewport` defaults to 64 px/beat and zooms between 4 and 1,000,000 px/beat.
`Viewport::x_to_beat` clamps to zero; `TimelineGrid::beat_for_x` does not. Both
guard the pixels-per-beat divisor.

## 7. Snapping and waveform mapping

Snapping is absolute, not delta-relative: the start time is snapped and the
shift is derived. Arranger times are already absolute. Audio events are stored
relative to the clip start, so the clip editor snaps `clip.range.start +
local_time` and subtracts the clip start. MIDI notes are stored in content
time, so the MIDI editor uses `clip.range.start - midi.content_start` as the
absolute offset. Drag deltas are always measured from the press origin; Shift
is the universal bypass and Alt remains accepted. The edit algebra receives
resolved times and never snaps.

Waveforms are read-only view data in an `Arc<WaveformCache>` keyed by `AssetId`.
Display samples are per-channel means. A cached peak hierarchy stores min/max
over power-of-four windows (4, 16, 64, ... up to the whole asset).
`draw_waveform` maps the requested `full_rect` linearly onto the source window,
intersects it with the visible clip, clamps to the asset's frame count, and
builds only visible geometry. Exact span peaks come from a range query that
consumes the largest aligned 4^n blocks first, then progressively smaller ones.
At 4096 or fewer samples per pixel the sample graph queries two spans per pixel
and fills the area without an extra stroke. Above that, far rendering queries
one span per pixel and draws one-pixel vertical lines from zero to the positive
and negative peaks instead of an area. Both modes draw a one-pixel zero line
across the visible rect. Sliced waveforms start after `silent_prefix_samples`; over-trimmed
events never stretch the source.


## Clip editor negative time

The arranger viewport clamps its start at zero. The audio and MIDI clip editors
use the unbounded viewport navigation and coordinate conversion so their
timeline can pan left of the zeroth bar. This keeps content placed before bar
zero visible and editable without changing the arranger's clamp.
