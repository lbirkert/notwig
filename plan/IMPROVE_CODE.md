# Improving the code structure

Review of the current implementation and the refactors applied after the audio
stability work. The goal is to keep `Workspace` focused on project policy while
giving independent concerns their own modules.

## Findings

- `crates/notwig-app/src/workspace.rs` was 1,666 lines and mixed project
  lifecycle, async engine builds, plugin discovery, asset I/O, clip editing,
  offline rendering, and transport updates. Most of those concerns do not need
  to share a file.
- Engine build state was spread over three fields (`engine_build`,
  `engine_rebuild_queued`, `engine_build_requests`) and a 40-line `poll` match
  with three copies of the queued-rebuild handling.
- The plugin catalog, scanner subprocess, cache path, and `PluginHost`
  constructor lived at the bottom of `workspace.rs` even though they are
  independent of a workspace.
- Audio asset loading, importing, relinking, peak computation, and the
  `PrepareAudioAsset` send lived among unrelated clip and mixer commands.
- The settings and render windows repeated the label/`ComboBox` layout. The
  audio settings view was assembled inline in `gui.rs`.
- Large UI modules remain: `panel/clip_editor.rs` (1,783 lines),
  `arranger.rs` (1,512 lines), and `shell.rs` (1,386 lines).

## Changes applied

- [x] Extract the async engine build lifecycle into
  `crates/notwig-app/src/engine_build.rs`.
  - `EngineBuildRequest::build` owns the plan compile and schedule/asset setup.
  - `EngineBuild` owns the worker receiver, queued flag, and request count.
  - `Workspace` keeps only the install/transport-sync policy and status text.
- [x] Extract plugin discovery into `crates/notwig-app/src/plugins.rs`.
  - `PluginCatalog`, `new_plugin_host`, the cache path, and `PluginEntry`
    conversion no longer live in `workspace.rs`.
- [x] Extract the audio asset pipeline into
  `crates/notwig-app/src/workspace/assets.rs`.
  - Loading, importing, relinking, peaks, and `PrepareAudioAsset` are one
    cohesive module; clip insertion remains with the arranger commands.
- [x] Add shared form helpers in `crates/notwig-ui/src/form.rs`.
  - `section_heading` and `select_row` replace repeated label/`ComboBox`
    blocks in the settings and render windows. Driver/device/sample-rate
    side effects stay with the settings page.
- [x] Move audio settings view construction into
  `crates/notwig-app/src/audio.rs::settings_view`.
  - `gui.rs` now only supplies the driver options, current settings, error, and
    warning condition.

## Deferred opportunities

- Split `panel/clip_editor.rs` and `arranger.rs` by view, interaction, and
  lowering. Both files already use shared timeline helpers, so the next split
  should follow those seams rather than introducing new state.
- Extract an `AudioSettingsDraft` type from `settings.rs`. Driver changes
  currently reset the device and sample rate across three UI closures; that
  policy belongs on the draft type.
- Add a shared popup-window builder for settings, render, and future dialogs.
- Finish the remaining `plan/AUDIO.md` items: plugin load/activate timeouts,
  an import worker with allocation-free resampling and streaming peaks,
  streaming export, and FLAC/OGG coverage.

## Verification

- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
