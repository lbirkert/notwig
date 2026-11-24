# Workspace Overview

The window is split into a fixed app bar, a transport toolbar, a central
arranger, an optional bottom editor, and a status bar.

```text
┌──────────────────────────────────────────────────────────────┐
│ project card             NOTWIG              native controls  │
├──────────────────────────────────────────────────────────────┤
│ FILE  transport  tempo  time signature  position  meter        │
├───────────────┬──────────────────────────────────────────────┤
│ info panel    │ arranger                                     │
│ (optional)    │  track headers | timeline and clip lanes      │
├───────────────┴──────────────────────────────────────────────┤
│ device editor or clip editor (optional)                      │
├──────────────────────────────────────────────────────────────┤
│ status bar                                                   │
└──────────────────────────────────────────────────────────────┘
```

| Region | What it shows | Read more |
| --- | --- | --- |
| **App bar** | Project card, centered Notwig title, native window controls. | [App bar and top bar](#app-bar-and-top-bar) |
| **Top bar** | File menu, transport, tempo, time signature, position, CPU load, master meter. | [App bar and top bar](#app-bar-and-top-bar) |
| **Info panel** (left) | Selected track, clip, device, time range, or file. | [Info panel](#info-panel) |
| **Arranger** (centre) | Track headers, timeline, clip lanes, master track. | [Arranger](#arranger), [Arranger guide](04_arranger.md) |
| **Bottom editor** | Device chain, audio clip editor, or MIDI editor — one at a time. | [Bottom editors](#bottom-editors), [Devices](07_devices.md) |
| **Status bar** | Info/editor toggles and the current device-editor button. | [Status bar](#status-bar) |

## App bar and top bar

The app bar is always visible. It shows the project card on the left, derived
from the saved bundle file name and marked with `*` while the project is dirty,
and the Notwig mark and `NOTWIG` title centered. The card's close button closes
the current project and returns to an empty session. On macOS the native window
buttons remain visible next to the project card; on other platforms they stay
in the system title bar.

The transport toolbar sits directly below the app bar:

| Area | What it does |
| --- | --- |
| **FILE** | Icon-labelled entries for new, open, save, save as, import audio, audio settings, and render. |
| **Play / pause** | Starts playback or pauses at the current position. |
| **Stop** | Stops playback and returns to the play start. |
| **Record** | Starts playback and recording, or toggles recording while playing. |
| **Loop** | Turns the transport loop on or off. |
| **Metronome** | Turns the click on or off. |
| **CPU load** | Shows engine load. Click it to open the DSP analysis window. Right-click the middle engine area to stop or restart the audio engine. |
| **Tempo** | Global BPM. Audio clips keep their sample duration when tempo changes. |
| **Time signature** | Global numerator and denominator. |
| **Position** | Bars and beats on the first line, minutes and seconds on the second. |
| **Master meter** | Output level of the master track. |

Tempo, time signature, loop, and metronome state are stored in the project.
Shortcut bindings and audio device settings are stored per user.

## Arranger

The arranger contains the track headers and the timeline.

- Click a track header to select a track.
- Cmd-click or Shift-click to add tracks or select a range.
- Drag a track name to reorder it or move it into a group.
- Use the small triangle beside a group to collapse or expand its children.
- Drag the volume fader in a track header to change level.
- Click **M** to mute and **S** to solo. Solo respects group ancestry and
  descendants.
- Double-click a track header to open its device editor.
- Right-click a track header or lane for track and clip actions.
- Click the **+ Add track** row to create a track. Right-click it to choose the
  track kind.

The master track sits below the other tracks: at the bottom of the arranger
while the lane list fits, and pushed down into the scroll flow once the list
grows beyond the visible area.

Drag the vertical boundary between the track headers and the timeline to resize
the arranger sidebar. Clip title headers, MIDI note previews, and audio
waveforms stay visible much further into the zoomed-out range than before.

## Info panel

The info panel shows the current selection and mirrors the relevant controls.
It is toggled from the status bar.

- **Track** shows name, color, comment, active state, devices, mute, solo, pan,
  and volume with a peak meter.
- **Clip** shows an editable name, a color picker with an inherit option, the
  enable/disable power button, editable start/end/length fields, and duplicate
  or delete actions. Clip names may be empty and keep trailing spaces.
- **Device** shows the device name, type, bypass state, and parameters. The EQ
  device has a dedicated band view.
- **Time selection** shows start, end, length, and the selected lanes.
- **Files** shows the source asset for an audio clip.

When several objects are selected, the panel lists them and lets you choose the
primary selection.

## Bottom editors

Only one bottom editor is open at a time:

- **Device editor** shows the selected track's device chain, including nested
  Chain devices.
- **Clip editor** opens for the selected audio clip.
- **MIDI editor** opens for the selected MIDI clip.

The status bar toggles the device editor and the clip editor. Double-clicking a
clip opens the matching editor. Drag the top handle of a bottom editor downward
to close it, or drag it to resize.

The device editor and clip editor are mutually exclusive. Opening one closes
the other. With no valid selection, the editor shows a short placeholder
instead of controls.

## Status bar

The status bar contains three toggle buttons plus a device editor text button:

- **Info** toggles the left info panel.
- **Sliders** toggles the device editor.
- **Clip** toggles the clip editor.
- The text button on the right opens or closes the device editor.

## Timeline navigation

These gestures work in the arranger and both clip editors:

| Gesture | Result |
| --- | --- |
| Pinch or two-finger gesture | Zoom around the pointer. |
| Middle-drag horizontally | Pan the timeline. |
| Middle-drag vertically | Zoom the timeline. |
| Two-finger horizontal swipe | Pan the timeline. |
| Horizontal scrollbar | Pan the timeline. |
| **Cmd+0** | Fit the arrangement in the arranger. |

Snapping uses one shared setting for the arranger and both clip editors. Every
editor scrollbar ends with the same compact controls: the magnet toggles
snapping on or off and the grid label opens the grid settings. Adaptive mode
coarsens the grid as you zoom out and exposes finer levels as you zoom in,
while manual mode pins an explicit note value from **2/1** to **1/65536**.
Hold **Shift** or **Alt** while dragging to bypass snapping.

Next: [Projects and Recovery](02_projects_and_recovery.md).
