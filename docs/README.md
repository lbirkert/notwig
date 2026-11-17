# Notwig Implementation Docs

These documents describe the behaviour that is implemented in the current codebase. They are not the v0 target plan.

`plan/` remains the design backlog and target-contract set. It is stale in places (for example, it still lists undo/redo and tempo editing as deferred). When the plan and the implementation disagree, these documents describe what the code actually does.

## Documents

- [`EDITING.md`](./EDITING.md) — musical time, arranger geometry, viewport, snapping, selection, clip interactions, audio slice editing.
- [`FRONTEND.md`](./FRONTEND.md) — shell layout, transport, inspector/mixer, bottom-editor switching, focus, shortcuts, file drop, theme, UI-state ownership.
- [`ARRANGER.md`](./ARRANGER.md) — track headers, lanes, clip display, track ordering, ruler/play-start, navigation.
- [`DEVICES.md`](./DEVICES.md) — device rack, chain nesting, add-device browser, stock/plugin parameter UI, device inspector.
- [`CLIPS.md`](./CLIPS.md) — clip model, overlap policy, copy/paste/duplicate/split semantics, and the no-linked-clips decision.

Status: locked to the current implementation. Source module names are named in each document so future refactors can verify the contract.
