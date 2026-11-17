# UI Interaction (`notwig-ui`)

## 1. Input & Routing
- Pointer/keyboard/wheel events routed by focus/hover and deterministic hit-test z-order.
- Outcomes: `ΔS_ui`, and/or `Command` to Core, and/or non-RT requests (scan/load/editor open). Core is authority for acceptance.

## 2. Selection & Focus
- Typed ID selection sets with primary/anchor where needed; additive/toggle/range selection consistent across views.
- Focus independent from selection where required; stored in `S_ui` only.

## 3. Drag and Drop
`DnD = {source,payload,mode(Move|Copy|Insert),target,position} -> Cmd` on valid drop.
- Payloads (v0): clips (lane moves subject to auto-trim rules), devices (reorder/insert), browser items (assets/devices/presets) to compatible targets.
- `canDrop` predicate gates accept with visual feedback; drop commits a single logical Core operation.

## 4. Editing Gestures
- Click/double-click/drag/edge-drag/marquee; modifiers for fine adjust, duplicate, or alternate modes (defined per tool).
- Snapping/quantization: based on grid/zoom/context (musical grid for arranger/piano roll); affects emitted command values. v0 defaults to 1/16-note snap with a modifier (Alt/Option) to bypass; finer/coarser steps follow zoom, and triplet/other grids are deferred.
- Continuous drags may use throttled or preview+commit emission; final committed value representable in `S_doc`.

## 5. Feedback & Shortcuts
- Hover/active/valid-drop indicators, tooltips/readouts, cursors, progress/busy states for async ops.
- Context-scoped shortcut map `KeyChord -> Action -> Command|UIAction`; core actions keyboard-reachable where feasible.
- v0 baseline (provisional, macOS `Cmd` / other `Ctrl`):
  - `Space` play/pause; `Shift+Space` stop; `Delete`/`Backspace` delete selection; `Cmd+D` duplicate selected clip(s). Track/device duplication and copy/paste are deferred in v0.
  - `Cmd+N` new, `Cmd+O` open, `Cmd+S` save, `Cmd+Shift+S` save-as.
  - `Alt/Option` bypass snap; `Cmd+wheel` zoom; `Cmd+0` fit view.
  - No undo/redo shortcuts in v0.
