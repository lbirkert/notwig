# Testing

Stability is the main project constraint. This page records the tests that exist and the tests that should exist, grouped by level. It is a coverage map, not a schedule.

## Status values

| Status | Meaning |
| --- | --- |
| ✅ Implemented | The test exists and passes. |
| 🧪 Partial | Some coverage exists, but important cases are missing. |
| 📋 Planned | The test should be written. |
| 🚫 Not Planned | Out of scope, or only practical as a manual check. |

| Importance | Meaning |
| --- | --- |
| ⭐ | Low priority. |
| ⭐⭐ | Nice to have. |
| ⭐⭐⭐ | Useful, but not blocking. |
| ⭐⭐⭐⭐ | High priority for stability. |
| ⭐⭐⭐⭐⭐ | Essential. A failure here breaks core workflows. |

## Test levels

| Level | Purpose |
| --- | --- |
| Unit | Pure functions and small modules: time, IDs, parameter math, snapping, smoothing. |
| Integration | Contracts between crates: commands, persistence, engine rendering, plugin cache. |
| UI | Headless `egui` render and interaction tests: shell, arranger, audio editor, settings, shortcuts. |
| End-to-end | CLI and full workspace flows: create, import, edit, save, reopen, render. |
| Editing workflow | Cross-cutting checks for move, duplicate, split, trim, copy, paste, delete, and undo. |
| DSP stability | Numeric and realtime behavior across blocks, sample rates, long runs, and edge cases. |
| Performance and realtime safety | Callback constraints, load time, frame time, memory growth, and stress behavior. |
| Manual | Checks that need audio hardware, real plugin binaries, or long sessions. |

## Running tests

```sh
cargo test --workspace
cargo test -p notwig-core
cargo test -p notwig-engine --test render
cargo test -p notwig-ui --test audio_editor
cargo bench -p notwig-core
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

Plugin integration tests build a real truce plugin and are manual-only:

```sh
cargo install cargo-truce --version 6.3.0
scripts/test-plugins.sh          # CLAP/VST3/VST2, plus LV2 when lilv is installed
NOTWIG_PLUGIN_TEST_AU3=1 scripts/test-plugins.sh   # macOS AU v2 + AU v3
```

The script builds `crates/notwig-plugins/tests/fixtures/truce_test_plugin`,
sets `LV2_PATH`, and runs the ignored `plugin_integration` tests. Native editor
tests run only on macOS because the editor host is AppKit-only. Windows runs the
same script from Git Bash; LV2 is skipped there because the runner does not
install lilv.

`.github/workflows/plugin-integration.yml` is a manual-dispatch workflow that
runs the same script on Intel and Apple Silicon macOS, Linux, and Windows
runners. Its `runners` input takes a JSON array so other labels can be added.
AU v3 requires a Developer ID certificate: configure
`APPLE_DEVELOPER_ID_P12` and `APPLE_DEVELOPER_ID_PASSWORD` secrets, otherwise
the workflow skips AU v3 with a notice. Native editor window tests are skipped
by default in the workflow because opening AppKit/GPU windows can abort on
hosted runners; enable the `run_editor` input on a suitable macOS runner, or run
locally with `NOTWIG_PLUGIN_TEST_EDITOR=1 scripts/test-plugins.sh`.

Hosted macOS runners can start `WindowServer`, but they do not provide a stable
logged-in GUI/GPU session for third-party plugin editors. End-to-end window
tests therefore belong on a self-hosted macOS runner with an interactive
session (or on a developer machine); hosted CI still covers discovery, loading,
DSP, parameters, state, latency, and the editor-unavailable stub. Notwig has no
native editor host on Linux/Windows, so editor E2E coverage is macOS-only.

The suite lives under `crates/*/tests/`. There is one inline unit-test module
(`notwig-core/src/dsp/monitor.rs`), one Criterion benchmark harness
(`crates/notwig-core/benches/dsp.rs`), and no `cargo fuzz` target yet. Rows in the
tables below summarize test groups. One row can cover several test functions.

## End-to-end tests

| Test | Status | Importance | Notes |
| --- | --- | --- | --- |
| CLI demo creates a bundle and renders audio | ✅ Implemented | ⭐⭐⭐⭐⭐ | `notwig-app/tests/cli.rs::demo_creates_bundle_and_renders_audio` covers import, devices, save, and render. |
| CLI help, version, and bad commands | ✅ Implemented | ⭐⭐⭐⭐ | `cli.rs::help_lists_commands_and_versions` checks visible commands and version output; `cli.rs::unknown_command_is_rejected` checks clap error handling. |
| New project CLI creates a valid bundle | 📋 Planned | ⭐⭐⭐⭐ | The `new` path has no test. |
| Info CLI reports counts and load issues | ✅ Implemented | ⭐⭐⭐ | `cli.rs::demo_creates_bundle_and_renders_audio` checks `info` track and clip counts. Asset load issues are not asserted. |
| Render CLI honors start and end flags | 🧪 Partial | ⭐⭐⭐⭐ | The demo covers the default range. `--start` and `--end` are untested. |
| Save, reopen, and render a project | ✅ Implemented | ⭐⭐⭐⭐⭐ | `workspace.rs::create_clip_save_and_render`. |
| Device parameters change offline render | ✅ Implemented | ⭐⭐⭐⭐⭐ | `workspace.rs::device_parameters_affect_offline_render`. |
| MP3 import and render | ✅ Implemented | ⭐⭐⭐⭐ | `workspace.rs::imports_and_renders_mp3`. |
| Full GUI startup with a project | 🧪 Partial | ⭐⭐⭐⭐ | The shell renders headlessly. Full app startup with an audio device is untested. |
| Autosave recovery through the app | 🧪 Partial | ⭐⭐⭐⭐ | `notwig-app/tests/recovery.rs` covers lock/autosave offer rules, recover, discard, and corrupt autosaves. The modal itself has no headless render test. |
| Corrupt or missing project file handling | 📋 Planned | ⭐⭐⭐⭐⭐ | |
| Missing CLI paths | 📋 Planned | ⭐⭐⭐ | Path existence and shape are untested. |

## Integration and contract tests

| Test | Status | Importance | Notes |
| --- | --- | --- | --- |
| Document validation and template invariants | ✅ Implemented | ⭐⭐⭐⭐⭐ | `domain_flow.rs` plus `apply_validation.rs`, which checks every successful command leaves a valid document and failed commands do not mutate input. |
| Track hierarchy and recursive delete | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Clip overlap resolution | ✅ Implemented | ⭐⭐⭐⭐⭐ | `overlap_makes_space_and_preserves_both_fragments`. |
| Split with crossing notes and ID remap | ✅ Implemented | ⭐⭐⭐⭐⭐ | Core and app tests cover note IDs. |
| Audio clip range and gain edits | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Device placement rules | ✅ Implemented | ⭐⭐⭐⭐ | |
| Routing cycle rejection | ✅ Implemented | ⭐⭐⭐⭐ | |
| State delta affected sets | ✅ Implemented | ⭐⭐⭐⭐ | |
| Engine schedule extraction | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| MIDI note commands | ✅ Implemented | ⭐⭐⭐⭐⭐ | `notwig-core/tests/midi_notes.rs` covers create/upsert/delete, bounds and pitch validation, duplicate IDs, audio-clip rejection, ordering, and legacy commands. |
| Project JSON round-trip | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Edit algebra examples and property tests | ✅ Implemented | ⭐⭐⭐⭐⭐ | `edit_algebra.rs` has 14 example tests and 5 property tests. |
| Bundle save and open | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Autosave list and load | ✅ Implemented | ⭐⭐⭐⭐ | Numeric revision/timestamp ordering, atomic write, and corruption handling are covered. |
| Recovery failure retention | ✅ Implemented | ⭐⭐⭐⭐⭐ | `notwig-app/tests/recovery.rs` covers failed recover/discard keeping the lock and snapshots, and plugin-state reload after recovery. |
| Browser Save As staging | ✅ Implemented | ⭐⭐⭐⭐⭐ | `notwig-app/tests/web_save.rs` covers manifest-last ordering and failed-stage/failed-commit retention. |
| WAV, AIFF, and MP3 decode and resample | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| FLAC and OGG decode and resample | 🧪 Partial | ⭐⭐⭐ | Probe and export round-trips are tested. Import-decode coverage is still missing. |
| Audio probe and multi-format export | ✅ Implemented | ⭐⭐⭐⭐ | `probe.rs` covers header probing and duration for all formats; `export.rs` covers WAV/AIFF/FLAC/MP3/OGG writing, bit depth, dithering, and normalization. |
| Asset hashing and import dedupe | ✅ Implemented | ⭐⭐⭐⭐ | `import_hash.rs` covers content-addressed filenames, dedupe, and temp-file cleanup. |
| Engine render through master | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Stock device processing | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Nested chain processing | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Live parameter updates | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Engine control exchange and prepared runtimes | ✅ Implemented | ⭐⭐⭐⭐⭐ | `notwig-engine/tests/control.rs` covers the shared slot, fixed-frame sync interval, prepared-runtime installation, transport application, and playhead publication. |
| Plugin scan, cache, and blacklist | ✅ Implemented | ⭐⭐⭐⭐ | |
| Real plugin load and process (CLAP/VST3/VST2/LV2/AU) | ✅ Implemented | ⭐⭐⭐⭐⭐ | Manual `scripts/test-plugins.sh` builds the truce fixture and runs `notwig-plugins/tests/plugin_integration.rs`: discovery, load, capabilities, parameter updates, state round-trip, latency, re-prepare, silence, zero-frame blocks, and repeated instantiation. LV2 parameter/state gaps are asserted explicitly. |
| Plugin delay compensation across tracks | 🧪 Partial | ⭐⭐⭐⭐⭐ | `notwig-engine/tests/pdc.rs` covers flat and nested latent tracks and send-vs-direct phase coherence. Hosted plugin latency is queried through the vendored VST3/VST2/AU wrappers but is not exercised by the headless test suite. |

## Unit tests

| Test | Status | Importance | Notes |
| --- | --- | --- | --- |
| Typed ID round-trip and type separation | ✅ Implemented | ⭐⭐⭐ | |
| Rational normalization | ✅ Implemented | ⭐⭐⭐⭐ | |
| Tempo map round-trip | ✅ Implemented | ⭐⭐⭐⭐ | |
| Constant-power pan | ✅ Implemented | ⭐⭐⭐ | |
| Parameter range and normalization matrix | 🧪 Partial | ⭐⭐⭐⭐ | Stock tests exercise some values. No complete linear, log, bool, and enum matrix. |
| Smoothing convergence | 🧪 Partial | ⭐⭐⭐⭐ | One Tool gain test covers the smoothing path. |
| EQ coefficient edge cases | 🧪 Partial | ⭐⭐⭐⭐ | Unity bell and high-pass DC only. |
| Tool gain and polarity | ✅ Implemented | ⭐⭐⭐⭐ | |
| Graph compilation order | ✅ Implemented | ⭐⭐⭐⭐ | |
| Unknown plugin resolution | ✅ Implemented | ⭐⭐⭐ | |
| Extension ABI and WASM devices | ✅ Implemented | ⭐⭐⭐⭐ | `crates/notwig-ext/notwig-ext/tests/abi.rs` covers describe, panel output, DSP, and latency with inline WAT modules; `limiter_extension.rs` covers the built WASM and native targets. |
| Extension command contract | ✅ Implemented | ⭐⭐⭐⭐ | `crates/notwig-ext/notwig-ext-api/tests/command_contract.rs` covers manifest v2, `when` parsing, registration, and execution; `crates/notwig-ext/notwig-ext/tests/commands.rs` covers host resolution and WASM execution. |
| Command palette and string bindings | ✅ Implemented | ⭐⭐⭐⭐ | `crates/notwig-ui/tests/shortcuts.rs` covers F1 palette opening, registry precedence, extension defaults, round-trips, and headless rendering. |
| Extension command effects and undo | ✅ Implemented | ⭐⭐⭐⭐ | `crates/notwig-app/tests/workspace.rs` applies a note edit effect as one undo entry. |
| Extension panel frame validation | ✅ Implemented | ⭐⭐⭐⭐ | `crates/notwig-ext/notwig-ext-api/tests/panel_wire.rs` covers mesh, texture, clip, and action limits. |
| Extension ABI helpers | ✅ Implemented | ⭐⭐⭐ | `crates/notwig-ext/notwig-ext-api/tests/abi_helpers.rs` covers the const `DeviceStore` and handle lifecycle. |
| Extension CLI packaging and scaffolding | ✅ Implemented | ⭐⭐⭐⭐ | `crates/notwig-ext/cargo-notwig-ext/tests/cli.rs` covers manifest transformation, target-gated includes, default icons, and `new` scaffolding. |
| Extension index and config | ✅ Implemented | ⭐⭐⭐⭐ | `crates/notwig-ext/notwig-ext-api/tests/index_config.rs` covers index parsing/validation and legacy config migration. |
| Extension ZIP install | ✅ Implemented | ⭐⭐⭐⭐ | `crates/notwig-ext/notwig-ext/tests/install.rs` covers root/nested layouts, unsafe paths, ID mismatch, replacement, and uninstall. |
| Limiter, gate, and synth example devices | ✅ Implemented | ⭐⭐⭐⭐ | `crates/notwig-ext/notwig-ext-devices/tests/limiter.rs` covers the limiter spec, panel frame, lookahead transient catching, threshold limiting, and release recovery. `crates/notwig-ext/notwig-ext-devices/tests/gate.rs` covers the gate spec, panel frame, and open/close behavior. `crates/notwig-ext/notwig-ext-devices/tests/synth.rs` covers the synth spec, panel frame, oscillator modes, note on/off, and ADSR release. `crates/notwig-ext/notwig-ext-api/tests/panel_bridge.rs` covers guest frame generation and actions. `crates/notwig-ext/notwig-ext-api/tests/note_events.rs` covers note-event wire encoding. `crates/notwig-ext/notwig-ext/tests/limiter_extension.rs` loads the built wasm through the real host when present, covering the generated `#[notwig_extension]` ABI exports and synth note playback. |
| Edit lowering ID remap | 🧪 Partial | ⭐⭐⭐⭐⭐ | Covered through app tests, not isolated unit tests. |
| Snap and grid math | ✅ Implemented | ⭐⭐⭐⭐ | |
| Waveform peak mapping | ✅ Implemented | ⭐⭐⭐ | |
| Selection state rules | ✅ Implemented | ⭐⭐⭐⭐ | |
| History stack semantics | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Summary counts | ✅ Implemented | ⭐⭐ | |
| Shortcut config round-trip | ✅ Implemented | ⭐⭐⭐⭐ | |
| Audio settings config round-trip | ✅ Implemented | ⭐⭐⭐⭐ | `notwig-app/tests/audio_config.rs` covers round-trip, malformed/missing files, missing/extra fields, and device validation. |
| Asset content hash | ✅ Implemented | ⭐⭐⭐⭐ | `notwig-project/tests/import_hash.rs` asserts the stored filename and manifest hash are SHA-256 of the stored bytes. |
| Sample-rate conversion quality | 📋 Planned | ⭐⭐⭐⭐ | |
| Denormal and NaN handling | 📋 Planned | ⭐⭐⭐⭐⭐ | |
| Automation curve math | 📋 Planned | ⭐⭐⭐⭐⭐ | |
| Warp and stretch math | 📋 Planned | ⭐⭐⭐⭐⭐ | |

## UI tests

| Test | Status | Importance | Notes |
| --- | --- | --- | --- |
| Shell renders headlessly | ✅ Implemented | ⭐⭐⭐⭐⭐ | `notwig-ui/tests/shell.rs`; `shell_internal.rs` covers editor exclusivity and shortcut routing. |
| Arranger interaction | ✅ Implemented | ⭐⭐⭐⭐ | `notwig-ui/tests/arranger.rs` covers zoom/pan stability, clip creation rules, track drag/reorder, and context selection. |
| Timeline math and waveform mapping | ✅ Implemented | ⭐⭐⭐⭐ | `notwig-ui/tests/timeline.rs` covers coordinates, snapping, trim clamps, loop edges, and visible source windows. |
| View state and selection | ✅ Implemented | ⭐⭐⭐⭐ | `notwig-ui/tests/state.rs` covers selection modes, retention, and notification expiry. |
| Clipboard and edit helpers | ✅ Implemented | ⭐⭐⭐⭐ | `notwig-ui/tests/edit.rs` and `clip_chrome.rs`. |
| Widget and waveform primitives | ✅ Implemented | ⭐⭐⭐ | `notwig-ui/tests/widgets.rs`, `waveform.rs`, and `device_browser.rs`. |
| Arranger zoom and pan | ✅ Implemented | ⭐⭐⭐ | |
| Audio editor selection and move | ✅ Implemented | ⭐⭐⭐⭐⭐ | 49 tests in `audio_editor.rs`. |
| Audio editor trim and clip bounds | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Audio editor copy, cut, paste, duplicate, delete | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Drop target resolution | ✅ Implemented | ⭐⭐⭐⭐ | |
| Platform clipboard events | ✅ Implemented | ⭐⭐⭐⭐ | |
| Drag and drop without cursor events | ✅ Implemented | ⭐⭐⭐⭐ | |
| Audio file hover preview | ✅ Implemented | ⭐⭐⭐ | |
| Device info panel selection | ✅ Implemented | ⭐⭐⭐⭐ | |
| Settings page render | ✅ Implemented | ⭐⭐⭐⭐ | |
| Extension device panel | ✅ Implemented | ⭐⭐⭐⭐ | `notwig-ui/tests/extension_panel.rs` covers panel frame compositing, meter delivery, and invalid-frame handling. |
| Extension explorer | ✅ Implemented | ⭐⭐⭐⭐ | `notwig-ui/tests/extension_settings.rs` covers index/installed rendering and ZIP drop-to-install intents. |
| Shortcut settings page | ✅ Implemented | ⭐⭐⭐⭐ | |
| Shortcut focus and routing | ✅ Implemented | ⭐⭐⭐⭐ | |
| Editors mutually exclusive | ✅ Implemented | ⭐⭐⭐ | |
| Master track pinned | ✅ Implemented | ⭐⭐⭐ | |
| DSP analysis window render | ✅ Implemented | ⭐⭐ | |
| MIDI editor interaction | ✅ Implemented | ⭐⭐⭐⭐⭐ | `notwig-ui/tests/midi_editor.rs` covers open/render, held and automatic note previews, key highlighting, zero-width time selection, marquee expansion, notes outside the clip range, create/delete, keyboard nudging, collision-aware move/resize, vertical velocity editing, and copy/paste. `notwig-engine/tests/preview.rs` covers held and automatic preview note-offs. |
| Loop range edge resize | 🧪 Partial | ⭐⭐⭐ | Range math is tested; the egui edge hit-testing path is not. |
| Meter reset and peak hold | ✅ Implemented | ⭐⭐⭐⭐ | `notwig-app/tests/transport.rs` covers stop reset; `notwig-ui/tests/track_info.rs` covers peak hold and click reset. |
| Recovery modal render | 📋 Planned | ⭐⭐⭐ | The recovery load/discard behavior is tested at the workspace level. |
| Automation editor interaction | 📋 Planned | ⭐⭐⭐⭐⭐ | |
| Mixer view interaction | 🚫 Not Planned | ⭐⭐ | No mixer view in the core. |
| Marquee selection | 📋 Planned | ⭐⭐⭐ | |
| Device drag reorder | ✅ Implemented | ⭐⭐⭐⭐ | |
| Undo and redo across panels | 🧪 Partial | ⭐⭐⭐⭐⭐ | Document undo is tested. UI surface coverage is incomplete. |
| Startup without an audio device | 📋 Planned | ⭐⭐⭐⭐ | |
| Window scaling and high DPI | 📋 Planned | ⭐⭐ | |
| Native plugin editor window | 🧪 Partial | ⭐⭐⭐ | macOS manual path in `plugin_editor.rs`: open, size stability, host-to-editor parameter updates, state save/load, and reopen. Non-macOS builds assert the backend-unavailable stub. |

## Editing workflow tests

Editing has two surfaces: the arranger and the audio editor (the audio editor module). Both should share selection, snapping, destination clearing, undo grouping, and operation semantics. The tests below cover the shared behavior, the audio editor's container boundaries, and the gesture routing that decides between move and trim.

Use one shared fixture for both editors: two audio tracks, one audio clip with two slices, one slice extending before the container, one slice extending after it, and a MIDI clip with notes crossing the edit boundaries. That fixture makes parity failures visible and keeps the boundary cases in one place.

### Shared operation matrix

| Operation | Arranger | Audio editor | Importance | Notes |
| --- | --- | --- | --- | --- |
| Select one or more objects | ✅ Implemented | ✅ Implemented | ⭐⭐⭐⭐⭐ | Additive selection is covered in both editors. |
| Select a time range | ✅ Implemented | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Move an object within a lane | 🧪 Partial | ✅ Implemented | ⭐⭐⭐⭐⭐ | The audio editor move path is covered. Add a direct arranger drag test. |
| Move an object across lanes or tracks | 🧪 Partial | 🚫 Not Planned | ⭐⭐⭐⭐ | The audio editor has one lane. Add an arranger cross-track drag test. |
| Move a multi-object selection | 🧪 Partial | ✅ Implemented | ⭐⭐⭐⭐⭐ | Group offsets and clamping are covered. Cross-track group move is not implemented. |
| Move a time selection | ✅ Implemented | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Trim an object start | 🧪 Partial | 🧪 Partial | ⭐⭐⭐⭐⭐ | Leftward trim has no test in either editor. The audio editor currently moves instead of trimming. |
| Trim an object end | 🧪 Partial | ✅ Implemented | ⭐⭐⭐⭐⭐ | Audio editor end trim is covered. Add a direct arranger trim test. |
| Trim the container start | 🚫 Not Planned | 🧪 Partial | ⭐⭐⭐⭐⭐ | The arranger has no container. Leftward container trim is untested and currently broken. |
| Trim the container end | 🚫 Not Planned | 🧪 Partial | ⭐⭐⭐⭐⭐ | The arranger has no container. Rightward end trim is covered. Add leftward end trim and event-outside-container cases. |
| Copy an object selection | ✅ Implemented | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Copy a time selection | ✅ Implemented | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Cut an object selection | ✅ Implemented | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Cut a time selection | ✅ Implemented | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Paste an object selection | ✅ Implemented | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Paste a time selection | ✅ Implemented | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Duplicate an object selection | ✅ Implemented | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Duplicate a time selection | ✅ Implemented | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Delete an object selection | ✅ Implemented | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Delete a time selection | ✅ Implemented | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Split at the playhead | ✅ Implemented | 📋 Planned | ⭐⭐⭐⭐⭐ | The audio editor has no split command. Time-range delete is the current equivalent. Add event split for parity. |
| Clear the destination before paste or duplicate | ✅ Implemented | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Resolve overlaps after an edit | ✅ Implemented | 🧪 Partial | ⭐⭐⭐⭐⭐ | Arranger clearing is covered. Add audio editor slice overlap tests. |
| Undo and redo one gesture | 🧪 Partial | 🧪 Partial | ⭐⭐⭐⭐⭐ | Add one test per operation, not just one per editor. |
| Keep selection valid after an operation | 🧪 Partial | ✅ Implemented | ⭐⭐⭐⭐ | |
| Snap and Alt bypass | ✅ Implemented | 🧪 Partial | ⭐⭐⭐⭐ | Move snapping is covered. Add trim snapping tests. |
| Share one clipboard between editors | ✅ Implemented | ✅ Implemented | ⭐⭐⭐⭐ | `Workspace` owns an arranger/audio-clip clipboard; the audio editor publishes extracts with `UiIntent::SetClipboard`. Covered by `audio_editor_copy_replaces_the_arranger_clipboard` and `copying_in_the_audio_editor_uses_the_shared_clipboard`. |
| Reject cross-domain paste | ✅ Implemented | ✅ Implemented | ⭐⭐⭐⭐ | Both paste paths reject a foreign extract without changing the document. |

### Audio editor boundary and trim tests

| Test | Status | Importance | Notes |
| --- | --- | --- | --- |
| Drag an event past the clip end | 🧪 Partial | ⭐⭐⭐⭐⭐ | Lowering test `move_allows_events_past_the_clip_end` exists, but the UI gesture clamps to the container. Add a UI drag test. |
| Drag an event before the clip start | 🧪 Partial | ⭐⭐⭐⭐⭐ | Lowering test `events_can_move_before_the_clip_start` exists, but the UI gesture clamps to the container. Add a UI drag test. |
| Drag an event past both bounds in one gesture | 📋 Planned | ⭐⭐⭐⭐⭐ | |
| Drag an event outside the bounds and scroll | 📋 Planned | ⭐⭐⭐⭐ | |
| Render and select events outside the container | 🧪 Partial | ⭐⭐⭐⭐⭐ | Emission is covered. Add render and selection assertions. |
| Keep events outside the container silent during playback | 📋 Planned | ⭐⭐⭐⭐⭐ | Playback must clip to the container. |
| Move an event back inside the container | 📋 Planned | ⭐⭐⭐⭐ | |
| Trim an event start leftwards | 🧪 Partial | ⭐⭐⭐⭐⭐ | Lowering tests cover rightward start trim only. The gesture currently moves the event. |
| Trim an event start rightwards | ✅ Implemented | ⭐⭐⭐⭐⭐ | `trim_start_keeps_right_edge_fixed`. |
| Trim an event end leftwards | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Trim an event end rightwards | ✅ Implemented | ⭐⭐⭐⭐⭐ | `trim_end_extends_source_and_clamps_non_empty`. |
| Trim an event to the minimum length | ✅ Implemented | ⭐⭐⭐⭐⭐ | The non-empty clamp is covered. |
| Trim an event against source bounds | 🧪 Partial | ⭐⭐⭐⭐⭐ | Add exact sample-window assertions at both source edges. |
| Trim an unsliced audio clip | 🧪 Partial | ⭐⭐⭐⭐⭐ | Clip range and source window use a separate path. |
| Trim a sliced audio clip | 🧪 Partial | ⭐⭐⭐⭐⭐ | Add slice-window and fade-reset assertions. |
| Trim with snap on and off | 📋 Planned | ⭐⭐⭐⭐ | |
| Trim a time selection edge | 📋 Planned | ⭐⭐⭐⭐ | |
| Trim the container start leftwards | 🧪 Partial | ⭐⭐⭐⭐⭐ | `clip_edge_drag_resizes_and_clamps` covers rightward start trim only. The gesture currently moves the clip. |
| Trim the container start rightwards | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Trim the container end leftwards | 📋 Planned | ⭐⭐⭐⭐⭐ | |
| Trim the container end rightwards | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Trim the container while events sit outside | 📋 Planned | ⭐⭐⭐⭐⭐ | |
| Keep content anchored when the container start moves | 🧪 Partial | ⭐⭐⭐⭐⭐ | Add timeline-anchor assertions. |
| Keep the opposite edge fixed during trim | ✅ Implemented | ⭐⭐⭐⭐⭐ | Algebra tests cover this. |
| Never invert the span during trim | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Never stretch audio during trim | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Resolve overlaps after trim | 📋 Planned | ⭐⭐⭐⭐ | |

### Gesture routing tests

| Test | Status | Importance | Notes |
| --- | --- | --- | --- |
| Object body starts a move, not a trim | 🧪 Partial | ⭐⭐⭐⭐⭐ | Move tests exist. Add a hit-target test at the boundary. |
| Object edge starts a trim, not a move | 📋 Planned | ⭐⭐⭐⭐⭐ | This is the reported leftward trim failure. |
| Container edge starts a container trim, not a move | 📋 Planned | ⭐⭐⭐⭐⭐ | This is the reported clip trim failure. |
| Time-selection edge wins over an object edge | 📋 Planned | ⭐⭐⭐⭐ | |
| Pressing an object edge clears the time selection | 📋 Planned | ⭐⭐⭐⭐ | |
| Pressing an empty lane starts a time selection | ✅ Implemented | ⭐⭐⭐⭐ | A click creates a zero-width time selection in both editors; `clicking_empty_lane_creates_a_zero_width_time_selection` covers the audio editor. |
| Drag delta is measured from the press origin | ✅ Implemented | ⭐⭐⭐⭐⭐ | `pointer_delta_tracks_the_press_origin`. |
| New gesture uses a new origin | 📋 Planned | ⭐⭐⭐⭐ | Catches ratcheting after a clamp. |
| Snap is idempotent | 📋 Planned | ⭐⭐⭐⭐ | |
| Alt bypass returns raw time | ✅ Implemented | ⭐⭐⭐⭐ | Move bypass is covered. |
| Zoom keeps the beat under the pointer fixed | ✅ Implemented | ⭐⭐⭐⭐ | |
| Minimum selection clamps instead of collapsing | 📋 Planned | ⭐⭐⭐⭐ | |
| Shared beat strip maps to the same beat in both editors | 📋 Planned | ⭐⭐⭐⭐ | |
| Event edits never emit a container range change | 🧪 Partial | ⭐⭐⭐⭐⭐ | `emit_keeps_events_outside_the_clip_range` covers emission. Add assertions for move, trim, paste, and duplicate. |

### Editing invariants

| Invariant | Status | Importance | Notes |
| --- | --- | --- | --- |
| Move preserves object length and content | ✅ Implemented | ⭐⭐⭐⭐⭐ | Algebra property tests. |
| Time move preserves span length | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Cut equals copy then delete | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Trim keeps the opposite edge fixed | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Trim never inverts the span | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Trim never stretches audio | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Erase never changes content outside its span | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Destination content is authoritative | 🧪 Partial | ⭐⭐⭐⭐⭐ | App tests cover paste and duplicate. Add a property test. |
| Event edits never change the container | 🧪 Partial | ⭐⭐⭐⭐⭐ | Add per-operation assertions. |
| Operations are deterministic | ✅ Implemented | ⭐⭐⭐⭐⭐ | Property test. |
| Placed objects receive fresh IDs | 🧪 Partial | ⭐⭐⭐⭐⭐ | App tests cover IDs. Add a property test. |
| One undo group per gesture | 🧪 Partial | ⭐⭐⭐⭐⭐ | `apply_batch` covers some paths. Add per-editor gesture tests. |
| Selection stays valid after undo | 📋 Planned | ⭐⭐⭐⭐ | |
| Both editors produce equivalent results for the same operation | 📋 Planned | ⭐⭐⭐⭐⭐ | Shared fixtures and a parity assertion. |

### Other editing operations

| Test | Status | Importance | Notes |
| --- | --- | --- | --- |
| Marquee selection | 📋 Planned | ⭐⭐⭐ | Arranger only. |
| Cross-track multi-clip drag | 📋 Planned | ⭐⭐⭐⭐ | |
| Ripple delete and insert | 📋 Planned | ⭐⭐⭐⭐ | |
| Fade and gain editing | 📋 Planned | ⭐⭐⭐⭐ | Audio editor and arranger. |
| Crossfade creation | 📋 Planned | ⭐⭐⭐⭐ | |
| Clip loop and repeat editing | 📋 Planned | ⭐⭐⭐⭐ | |
| Reverse audio | 📋 Planned | ⭐⭐ | |
| Time-stretch and pitch edits | 📋 Planned | ⭐⭐⭐⭐⭐ | Depends on warping. |
| Create a clip at the playhead | ✅ Implemented | ⭐⭐⭐⭐ | |
| Create an audio event from a file drop | ✅ Implemented | ⭐⭐⭐⭐ | |
| Insert an audio event from the current source | ✅ Implemented | ⭐⭐⭐⭐ | |
| Delete the last event and keep the clip in event mode | ✅ Implemented | ⭐⭐⭐ | |

### Known regression tests

| Regression | Status | Importance | Notes |
| --- | --- | --- | --- |
| Drag an audio event before the clip start | 📋 Planned | ⭐⭐⭐⭐⭐ | User report. The test should fail before the interaction fix. |
| Drag an audio event after the clip end | 📋 Planned | ⭐⭐⭐⭐⭐ | User report. |
| Trim an audio event start leftwards | 📋 Planned | ⭐⭐⭐⭐⭐ | User report. The gesture currently moves the event. |
| Trim an audio clip start leftwards | 📋 Planned | ⭐⭐⭐⭐⭐ | User report. The gesture currently moves the clip. |
| Trim at the clip start boundary chooses trim over move | 📋 Planned | ⭐⭐⭐⭐⭐ | Hit-target precedence. |
| Trim at the clip end boundary chooses trim over move | 📋 Planned | ⭐⭐⭐⭐⭐ | Hit-target precedence. |
| Trim at high and low zoom | 📋 Planned | ⭐⭐⭐⭐ | Edge hit size must stay usable. |
| Trim after scrolling | 📋 Planned | ⭐⭐⭐ | |
| Move and trim do not ratchet | 📋 Planned | ⭐⭐⭐⭐ | Use press-origin deltas. |
| Alt bypass applies to trim | 📋 Planned | ⭐⭐⭐ | |
| One undo entry per drag | 🧪 Partial | ⭐⭐⭐⭐⭐ | Some operations are covered. Add drag grouping tests. |

## DSP stability tests

| Test | Status | Importance | Notes |
| --- | --- | --- | --- |
| Block-size invariance | ✅ Implemented | ⭐⭐⭐⭐⭐ | `block_size_does_not_change_stock_audio`. |
| Offline and realtime parity | 🧪 Partial | ⭐⭐⭐⭐⭐ | Both paths use the same renderer. A direct comparison test is missing. |
| Offline render determinism | 📋 Planned | ⭐⭐⭐⭐⭐ | Render twice and compare bit patterns or a tight tolerance. |
| Sample-rate matrix | 📋 Planned | ⭐⭐⭐⭐⭐ | 44.1, 48, 88.2, 96, and 192 kHz. |
| Long-run stability | 📋 Planned | ⭐⭐⭐⭐⭐ | Hours of playback with meter and drift checks. |
| Denormal and NaN handling | 📋 Planned | ⭐⭐⭐⭐⭐ | Inject silent tails, extreme values, and bad plugin output. |
| Parameter smoothing and zipper noise | 🧪 Partial | ⭐⭐⭐⭐ | One Tool gain test covers smoothing. |
| EQ stability at extreme settings | 🧪 Partial | ⭐⭐⭐⭐ | Add gain, Q, and frequency sweeps at every supported rate. |
| Chain dry/wet stability | 🧪 Partial | ⭐⭐⭐⭐ | Nested chain processing is covered. Dry/wet extremes are not. |
| Loop boundary and tails | 🧪 Partial | ⭐⭐⭐⭐ | Loop wrap, start-after-loop behavior, and crossing are tested. Reverb tails and plugin reset are not. |
| Tempo change with sample-locked audio | ✅ Implemented | ⭐⭐⭐⭐⭐ | `transport.rs` covers plan updates; `notwig-core/tests/tempo_audio.rs` covers adjacent-clip overlap resolution and unsafe-overlap rejection. |
| Audio events outside the clip container | ✅ Implemented | ⭐⭐⭐⭐⭐ | Core validation, engine clipping, and the audio editor gesture are covered. |
| PDC across tracks and nested groups | ✅ Implemented | ⭐⭐⭐⭐⭐ | `notwig-engine/tests/pdc.rs` covers flat and nested latent tracks. |
| PDC with sends | ✅ Implemented | ⭐⭐⭐⭐⭐ | Send and direct paths stay phase-coherent in `pdc.rs`. |
| Plugin latency reporting | ✅ Implemented | ⭐⭐⭐⭐⭐ | The vendored VST3/VST2/AU wrappers query latency; capability defaults and PDC plumbing are tested. |
| Xrun detection and recovery | 📋 Planned | ⭐⭐⭐⭐⭐ | |
| Audio callback allocation and lock checks | 🧪 Partial | ⭐⭐⭐⭐⭐ | The control slot uses `try_lock`; prepared swaps, meter publication, and CLAP/LV2 event-pointer scratch are reused. VST3 event buffers and external plugin code still need allocator instrumentation. |
| Seek and stop reset behavior | 🧪 Partial | ⭐⭐⭐⭐ | Stop note-off is tested. Plugin and effect tails are not. |
| Meter accuracy | 📋 Planned | ⭐⭐⭐⭐ | Peak, RMS, and hold behavior. |
| Mono and multichannel asset handling | 🧪 Partial | ⭐⭐⭐ | Mono to stereo is tested. More than two channels is partial. |
| Fade math at boundaries | 📋 Planned | ⭐⭐⭐⭐ | |

## Plugin hosting tests

| Test | Status | Importance | Notes |
| --- | --- | --- | --- |
| Scan, cache, and blacklist | ✅ Implemented | ⭐⭐⭐⭐ | |
| Real VST3 load and process | ✅ Implemented | ⭐⭐⭐⭐⭐ | Manual truce fixture via `scripts/test-plugins.sh`. |
| Real VST2 load and process | ✅ Implemented | ⭐⭐ | Manual truce fixture via `scripts/test-plugins.sh`. |
| Real CLAP load and process | ✅ Implemented | ⭐⭐⭐⭐ | Manual truce fixture via `scripts/test-plugins.sh`. |
| Real LV2 load and process | ✅ Implemented | ⭐⭐⭐ | Manual truce fixture; parameter/state support is still absent and asserted as an explicit gap. |
| Real AU v2/v3 load and process | 🧪 Partial | ⭐⭐⭐⭐⭐ | macOS manual tests exist; AU discovery requires installing the fixture. AU v3 async-only instantiation remains unwired. |
| Parameter enumeration and updates | ✅ Implemented | ⭐⭐⭐⭐ | Covered for CLAP/VST3/VST2/AU through the fixture. VST3/VST2 expose normalized values; CLAP/AU expose native units. |
| Native editor open and parameter sync | 🧪 Partial | ⭐⭐⭐ | macOS manual path; open/size/state/interaction covered, synthetic mouse input is not. |
| Plugin state save and restore | ✅ Implemented | ⭐⭐⭐⭐⭐ | Round-trip, wrong-UID rejection, and corrupt-blob safety covered for CLAP/VST3/VST2/AU. |
| Plugin latency query | ✅ Implemented | ⭐⭐⭐⭐⭐ | VST3, VST2, and AU wrappers expose latency to the adapter; the fixture advertises non-zero latency and tests per-format reporting. |
| Plugin MIDI output | 📋 Planned | ⭐⭐⭐ | |
| Sidechain and multi-out | 📋 Planned | ⭐⭐⭐⭐ | |
| Out-of-process crash isolation | ✅ Implemented | ⭐⭐⭐⭐⭐ | Native audio, plugin DSP, and plugin editor windows run in a supervised `engine-host` child process. Killing the child leaves the UI alive and shows the restart overlay. `notwig-plugins/tests/host.rs` covers metadata probe ordering and failure blocking; protocol framing and remote-control tests cover the new boundary. Per-plugin process isolation settings remain planned. |
| CLAP hosting | 🧪 Partial | ⭐⭐⭐⭐ | The adapter is enabled on native macOS builds; no real-plugin integration test. |
| LV2 hosting | 🧪 Partial | ⭐⭐⭐ | The adapter is enabled on native macOS builds with system `lilv`; no real-plugin integration test. |
| AU v3 hosting | 🧪 Partial | ⭐⭐⭐ | Registry scan/load is enabled on macOS; no real-plugin integration test. |
| VST2 hosting | 🧪 Partial | ⭐ | The adapter is enabled on native macOS builds; no real-plugin integration test. |

## Persistence and recovery tests

| Test | Status | Importance | Notes |
| --- | --- | --- | --- |
| Save and open round-trip | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Autosave list and load | ✅ Implemented | ⭐⭐⭐⭐⭐ | |
| Autosave interval and dirty gating | 🧪 Partial | ⭐⭐⭐⭐ | Failure retention is tested; the 60-second cadence and dirty gating still need a clock seam. |
| Crash recovery with `session.lock` | ✅ Implemented | ⭐⭐⭐⭐⭐ | `notwig-project/tests/bundle.rs` and `notwig-app/tests/recovery.rs` cover lock lifecycle, offer rules, recover, discard, and corrupt autosaves. |
| Schema migration | 📋 Planned | ⭐⭐⭐⭐⭐ | |
| Missing asset load issues | 🧪 Partial | ⭐⭐⭐⭐ | Issues are reported. No test asserts the missing-asset path. |
| Asset hash verification | 📋 Planned | ⭐⭐⭐⭐ | |
| Asset garbage collection | 📋 Planned | ⭐⭐⭐ | |
| Corrupt project file | 📋 Planned | ⭐⭐⭐⭐⭐ | |
| Atomic save under failure | 📋 Planned | ⭐⭐⭐⭐⭐ | |
| Sample-rate change with assets | 🧪 Partial | ⭐⭐⭐⭐ | Undo is tested. Resampling is not. |
| Recent projects | 🧪 Partial | ⭐⭐ | Registry and menu data are covered indirectly; no headless menu test. |
| Project templates | 📋 Planned | ⭐⭐ | |

## Performance and realtime safety tests

| Test | Status | Importance | Notes |
| --- | --- | --- | --- |
| Device process status handling | ✅ Implemented | ⭐⭐⭐⭐⭐ | `notwig-engine/tests/status.rs` covers overload sanitizing/fault reporting, silence clearing, and low-latency prepare failures. |
| Audio callback has no blocking locks | 📋 Planned | ⭐⭐⭐⭐⭐ | |
| Bounded engine command queue | 🚫 Not Planned | ⭐⭐⭐⭐⭐ | No command queue by design; the app swaps prepared `EngineData`. |
| Large project load and render | 📋 Planned | ⭐⭐⭐⭐ | |
| Many tracks and devices stress | 📋 Planned | ⭐⭐⭐⭐⭐ | |
| Plugin scan does not block the UI | 🧪 Partial | ⭐⭐⭐⭐ | A child process exists. No timing test. |
| UI frame time on a large arrangement | 📋 Planned | ⭐⭐⭐ | |
| Memory growth over a long session | 📋 Planned | ⭐⭐⭐⭐⭐ | |
| Xrun recovery | 📋 Planned | ⭐⭐⭐⭐ | |

## Manual and exploratory tests

| Test | Status | Importance | Notes |
| --- | --- | --- | --- |
| macOS audio device playback | 🧪 Partial | ⭐⭐⭐⭐⭐ | Manual smoke test each release. |
| Real plugin editor on macOS | 🧪 Partial | ⭐⭐⭐ | `plugin_editor.rs` via `scripts/test-plugins.sh` covers open, size, parameter state, and reopen for installed fixture formats. Synthetic mouse input and multi-window stress are still manual. |
| Audio device hot-plug | 📋 Planned | ⭐⭐⭐ | |
| Multi-hour session | 📋 Planned | ⭐⭐⭐⭐⭐ | |
| Finder drag and drop with multiple files | 🧪 Partial | ⭐⭐⭐ | Single and batch paths are partly covered. |
| OS clipboard interoperability | 🧪 Partial | ⭐⭐⭐ | Platform clipboard events are covered. |
| High DPI and window scaling | 📋 Planned | ⭐⭐ | |
| Release build smoke test | 📋 Planned | ⭐⭐⭐⭐ | |

## Next tests

| Test | Status | Importance | Notes |
| --- | --- | --- | --- |
| Editing consistency matrix | 📋 Planned | ⭐⭐⭐⭐⭐ | One fixture per operation across the arranger and audio editor. |
| Boundary and trim regression suite | 📋 Planned | ⭐⭐⭐⭐⭐ | Events outside the container, leftward trim, hit-target precedence, and zoom. |
| PDC and plugin latency suite | 🧪 Partial | ⭐⭐⭐⭐⭐ | Tracks, nested groups, sends, bypassed devices, and inactive tracks are covered. Hosted plugin latency is queryable through the vendored wrappers. |
| Real plugin host integration in CI | 📋 Planned | ⭐⭐⭐⭐⭐ | The truce fixture and manual runner exist. CI still needs `cargo-truce`, platform SDKs, and an installed AU fixture. |
| Engine realtime safety checks | 📋 Planned | ⭐⭐⭐⭐⭐ | Allocation and lock behavior; the prepared-data swap replaces a command queue. |
| DSP numeric stability suite | 📋 Planned | ⭐⭐⭐⭐⭐ | NaN, denormals, extremes, and the sample-rate matrix. |
| Project recovery and migration suite | 🧪 Partial | ⭐⭐⭐⭐⭐ | Autosave recovery and corrupt autosaves are covered. Corrupt manifests and schema migration are not. |
| Long-run soak test | 📋 Planned | ⭐⭐⭐⭐⭐ | Multi-hour playback with meters and plugins. |
| Edit lowering property tests | 📋 Planned | ⭐⭐⭐⭐ | IDs, ordering, and undo grouping. |
| UI render matrix | 📋 Planned | ⭐⭐⭐ | Panels, sizes, focus, and shortcuts. |
| Performance budgets | 📋 Planned | ⭐⭐⭐⭐ | Load time, render time, and UI frame time. |

## Gaps that matter most

- Editing tests cover many operations, but not every operation in [`Editing`](03_editing.md). The arranger and audio editor disagree on events outside the container and leftward trim. Add the boundary, hit-target, and parity tests before changing the interaction code.
- DSP tests cover stock processing and block sizes. They do not cover long runs, sample-rate changes, denormals, or NaN input.
- PDC for tracks, nested groups, sends, bypasses, and inactive tracks is tested. Hosted plugin latency is queried but real plugin hosting is not exercised headlessly.
- Autosave recovery and session locks are tested. Schema migration and corrupt project manifests are not.
- Realtime safety is not measured. Add allocation and lock checks before treating the engine as stable.
