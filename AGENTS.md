# AGENTS.md

## Project gist

Notwig is an open-source, feature-light digital audio workstation built around a
small number of carefully designed, extensible features. It is not a Bitwig
clone or a feature-parity project; Bitwig is a reference for interaction and
workflow ideas. The project is in the implementation phase, not the planning
phase.

- **Priorities:** extensibility, simplicity, few high-quality features, stable
  and correct real-time DSP.
- **Stack:** Rust, Cargo workspace, `egui` + `wgpu`, CPAL, `truce-rack` for
  VST3/AU hosting.
- **Platform:** macOS first; platform-specific code sits behind adapters.
- **Implemented today:** project bundles, autosave/recovery with a startup
  recovery modal, undo/redo, tracks, Arranger, MIDI/audio clips, audio clip
  event editing, a MIDI editor, device chains with stock EQ/Tool/Chain,
  VST3/AU hosting, WASM extension devices and panels with per-extension trust
  levels, an extension explorer with index/ZIP installs and uninstall,
  render mixdown, persisted audio settings, customizable keyboard shortcuts,
  and DSP load analysis.
- **Not implemented yet:** audio/MIDI recording, live input/monitoring,
  automation editing, general extension drawing hooks, Clip Launcher/scenes,
  comping/takes, warping, tempo/time-signature automation, modulation/macros
  runtime, stock instruments, Windows/Linux builds, collaboration, and
  out-of-process plugin hosting.

## Where to start

Read the stable documentation first. It is an mdBook under `docs/`, ordered by
audience with `<number>_` prefixes:

- [`docs/00_introduction.md`](docs/00_introduction.md) — what Notwig is and the
  documentation paths.
- [`docs/01_users/`](docs/01_users/) — user guide for the application.
- [`docs/02_extensions/`](docs/02_extensions/) — extension package, device, panel,
  and distribution docs.
- [`docs/03_development/00_architecture.md`](docs/03_development/00_architecture.md)
  — layers, state flow, threading, real-time rules, and invariants.
- [`docs/03_development/01_code_structure.md`](docs/03_development/01_code_structure.md)
  — crate/module map, extension recipes, and structural hotspots.
- [`docs/03_development/03_editing.md`](docs/03_development/03_editing.md) —
  shared editing algebra and adapter contracts. Read before changing arranger or
  clip editor operations.
- [`docs/03_development/04_editing_testing.md`](docs/03_development/04_editing_testing.md)
  — testing strategy for editing behavior.
- [`docs/03_development/06_roadmap.md`](docs/03_development/06_roadmap.md) —
  current features, candidate next milestones, and deferred scope.

Each crate has a `README.md` with its exact purpose, owned concerns, module map,
and boundaries. Start there when working inside a crate.

## Code style

### File organization

1. **One primary responsibility per file.** A file may contain closely related
   helpers, but it should not mix unrelated concerns just because they are
   convenient to keep together. If a file serves several distinct ideas, split
   it by concept.
2. **Split large files at seams, not at line counts.** File size is a signal,
   not a target. When a file becomes hard to navigate, identify the independent
   sub-systems inside it (state, interaction, rendering, I/O, lowering) and
   extract modules with clear interfaces. Do not split mechanically into
   arbitrary fragments.
3. **Extract shared behavior when duplication is real.** If the same logic is
   needed in more than one place, create a shared abstraction unless the
   duplication is deliberate and cheaper to maintain. Prefer a small, deep
   interface that hides details over a heavily parameterized helper that makes
   every call site harder to read.
4. **Default to narrow visibility.** Keep local helpers private. Use
   `pub(crate)` for code that is shared across a crate. Use `pub` only for an
   intentionally designed API: cohesive, documented, hard to misuse, and stable
   enough that other crates can depend on it. Do not make an implementation
   detail public just because another module needs it.
5. **Tests belong in the tests folder. Sources in src.**

### Architecture and ownership

- Follow the dependency direction in [`docs/03_development/01_code_structure.md`](docs/03_development/01_code_structure.md).
  Lower layers must not depend on higher layers.
- Keep `notwig-core` free of I/O, UI, plugin loading, and real-time execution.
- Keep canonical project state in `ProjectDoc`. UI code emits `UiIntent`; it
  does not mutate the document directly.
- Keep reusable domain/DSP logic out of `notwig-app`. The app composes, wires,
  and applies policy.
- Use the shared editing algebra for arranger and editor operations. Do not add
  a second copy of copy/cut/paste/duplicate/trim logic.
- Shared boundary types belong in the lowest crate that needs them.

### State, data, and determinism

- Prefer pure functions and explicit state transitions over hidden global state.
- IDs are caller-supplied. Domain code does not allocate IDs or use randomness.
- Use ordered collections whenever iteration order affects behavior,
  persistence, or tests.
- Name values by domain meaning: musical time vs audio time, clip vs event,
  track vs chain vs device. Avoid ambiguous unitless names.
- Keep persisted data versioned and canonically encoded.

### Errors and real-time safety

- Return typed errors at boundaries (`CoreError`, `ProjectError`,
  `EngineError`, `PluginError`, ...).
- Do not panic on recoverable user/input/file/plugin failures. Panics are for
  internal invariant violations only.
- The audio callback path must not allocate, block, perform file or plugin I/O,
  load plugins, compile DSP, or run unbounded work. Report runtime problems via
  `RtEvent`/status rather than unwinding across the callback.
- Do not hold locks while doing heavy work. Build engines and load plugins off
  the UI/audio threads.

### UI code

- Reuse `widgets/`, `timeline/`, and `form.rs` helpers instead of copying layout,
  snapping, or interaction math.
- Views read `RuntimeView`/`ProjectDoc`; they emit intents. Behavior belongs in
  `Workspace` or the app layer.
- Keep view state ephemeral. Do not stash canonical project state in `NotwigUi`.
- Add a headless `egui` render test when adding a panel, window, or interaction.

### Tests and verification

- Put integration tests in `crates/*/tests/`, not in `src/`.
- Test the lowest useful layer first. Use property tests for algebra and
  deterministic math; use workspace tests for command/lowering/undo flows.
- UI tests use a headless `egui::Context` and `RawInput`. `notwig_ui::test_api`
  is test-only.
- Run the standard checks after code changes:

  ```sh
  cargo fmt --all
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  ```

- For documentation-only changes, verify links and referenced paths and run
  `mdbook build`; run code checks if any code or examples changed.

### Dependencies and documentation

- Check existing workspace libraries and patterns before adding a dependency.
- Review licenses, platform support, and real-time implications before adopting
  a new crate. Keep plugin-format-specific types behind adapters.
- Update the relevant crate README or docs page when behavior, boundaries, or
  public APIs change.
- Write comments for non-obvious reasons and invariants, not for restating what
  the code already says.

### Documentation

- Documentation is part of the change, not a follow-up. Update it in the same
  patch that alters user-visible behavior, an extension API, a crate boundary,
  a persisted format, or an architectural invariant.
- Keep the three audience sections intact: `01_users`, `02_extensions`, and
  `03_development`. Put user instructions in the user guide, author-facing API
  guidance in the extension guide, and design rationale in development docs.
- Keep technical pages current with the code. If the code and a development page
  disagree, fix the page (or the code) before merging. These pages lock the
  design in place.
- Use `<number>_` prefixes for page files and register every page in
  [`docs/SUMMARY.md`](docs/SUMMARY.md). Keep links relative to the page.
- For documentation-only changes, verify links and referenced paths, and run
  `mdbook build`. Run code checks when examples or code snippets change.

## Todo workflow

Before starting any task with more than one step, write a precise TODO list with
the todo tool. Each item should be an atomic, verifiable action ("add
`CommandPayload::X` and apply it", "add the popup and render test"), not a vague
area ("do UI").

- Keep exactly one item `in_progress` while work remains.
- Update statuses as you go; do not batch completions at the end.
- Mark an item completed only after its verification has actually happened
  (tests, checks, link validation, or a deliberate manual review).
- Add new items discovered during implementation instead of silently expanding
  scope.
- If blocked, keep the item `in_progress` and record the concrete blocker.
- Include verification/commit steps in the list when they are part of the task.
- For a genuinely single-step task, skip the list and just do the work.

## Git

- Commit only when the user explicitly asks for a commit.
- Before committing, inspect `git status`, `git diff`, and recent history; stage
  only the intended files and never commit secrets.
- Write a concise message that matches the repository style.
