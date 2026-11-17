# UI Views (`notwig-ui`, v0)

## 1. App Shell
`Shell = { TransportBar, MainWorkspace, SidePanels, StatusBar, Settings, Dialogs/Notifications }`
- Resizable layout via `SplitView` (sizes in `S_ui`).
- Transport controls show/command intent and reflect runtime playhead/status (`S_doc` + `S_rt`): Play/Pause, Stop (to loop start/0), loop toggle/range.
- Tempo/time-signature are displayed read-only in v0 (fixed per project); no tempo/time-signature editors.
- No metronome, recording, live-input, or count-in controls in v0.
- Settings/preferences surface renders the app-provided settings snapshot and emits `SettingsIntent`s (audio device/buffer/rate, plugin paths, theme); `notwig-app` validates/persists them.
- Notifications/dialogs surface missing assets (relink flow), plugin load/scan errors, unsupported state, and xrun/overload status.

## 2. Arranger View
- Projects tracks/lanes/clips, ruler/grid, loop range, playhead.
- Edits emit commands: create/move/trim/split/duplicate/delete/mute clips and basic note/audio interactions as applicable.
- Overlaps not shown as persisted state; Core enforces auto-trim (v0). No launcher, no warp/takes UI.

## 3. Mixer View
- Strip per visible track in deterministic order consistent with hierarchy/order.
- Volume/pan/mute/solo, sends, routing affordances, meters from `S_rt`; no arm/monitor controls in v0.
- Track selection synced by ID across views.

## 4. Device / Inspector Panel
- Context: selected track/device/chain.
- Shows `DeviceChain` in `DeviceContainer` and selected `DeviceView` (header + params/visuals).
- Actions: insert/remove/reorder/bypass devices via commands; show latency/status; open plugin editor (non-RT request).

## 5. Browser Panel (v0)
- Sources: stock FX devices, discovered plugins including hosted instruments (async), project/bundle audio assets, presets where available.
- Minimal interactions: list/search/filter and insert/drag to compatible targets (track chain, arranger lane as supported) with async load/scan states.
- No MIDI-file import in v0.

## 6. Editors
- **Piano Roll (MIDI clip):** grid by pitch/time, note create/move/resize/delete/velocity edit. Notes only: no CC/pitch-bend/program-change lanes; per-note expression and channel UI deferred.
- **Audio Clip:** waveform with non-destructive trim/fade/gain at project rate; no elastic warp or content looping.

## 7. Consistency
- `Selection` in `S_ui` as typed ID sets `{tracks, clips, notes, devices}` with primary/anchor as needed.
- Visible changes reflect accepted `StateDelta`/`S_rt` (transient gesture previews may be `S_ui` only).
