# Browser build plan

Status: planning.

Target: run Notwig at `https://notwig.lbirkert.com/app/`, with the existing
documentation site served from `https://notwig.lbirkert.com/`.

The browser build is not a port of the native application to a new UI toolkit.
It is the same `notwig-app`/`notwig-ui`/`notwig-engine` composition with the
platform edges replaced: browser storage instead of `std::fs`, browser file
pickers instead of `rfd`, WebAudio instead of CoreAudio, and browser
WebAssembly instead of `wasmtime` or native VST3/AU hosting.

## 1. Goal and first milestone

### Must work in the first browser milestone

- Create a new project and open/save projects as real directories in
  browser-owned storage (OPFS), with direct file and subfolder creation.
- When the File System Access API is available, open/save a project bundle in a
  user-chosen directory, with permission prompts after reload.
- Import a `.notwig` bundle from a chosen directory or `.notwig.zip`, and
  export/download a zip on browsers without directory access.
- Arranger, MIDI editor, audio clip editor, track/device editing, undo/redo.
- Stock EQ, Tool, and Chain devices through the existing DSP engine.
- Playback through WebAudio, transport, loop, meters, and DSP load reporting.
- Import WAV/AIFF audio from a file picker or drag/drop.
- Render mixdown and download the result.
- Autosave and recovery in OPFS or the user-chosen project directory.
- MIDI/audio editing and playback using assets already in the project.

### Explicitly out of scope for the first milestone

- VST3 and AU hosting. These formats cannot run in a browser.
- Native plugin editor windows.
- `wasmtime` extension devices. The browser has its own WebAssembly API, but
  the host surface has to be rewritten first.
- Background native threads. The first web release must work without
  `SharedArrayBuffer` or cross-origin isolation.
- CLI commands (`new`, `demo`, `render`, `info`, `scan-plugins`).
- OS-level session locks based on process IDs.
- The user-visible directory workflow in browsers without the File System
  Access API. Those browsers use OPFS plus zip import/export.
- Byte-for-byte POSIX-style atomic rename semantics. Browser file APIs do not
  guarantee the same primitive and need an explicit save strategy.

### Later browser milestones

- Extension devices through the browser WebAssembly API.
- Lower-latency AudioWorklet output where the host can provide
  cross-origin isolation.
- Web Worker based engine builds and audio analysis for large projects.
- Streaming/lazy asset loading instead of loading an entire bundle into memory.

## 2. Why the project does not build for the browser today

The lower layers are mostly good news. `notwig-core`, `notwig-dsp`, and most
of `notwig-engine` contain no I/O, no UI, and no native plugin loading. The
blockers are concentrated in `notwig-app`, `notwig-project`, `notwig-plugins`,
`notwig-ext`, and a few build-time dependencies.

### 2.1 Entry point and windowing

- `crates/notwig-app/src/gui.rs:24` calls `eframe::run_native` and
  `crates/notwig-app/src/gui.rs:16` constructs `eframe::NativeOptions`.
  `run_native` is the native path. The browser path is `eframe::WebRunner` plus
  `eframe::WebOptions`, started from a `wasm-bindgen` entry point.
- `crates/notwig-app/src/main.rs:42` reads process arguments, `:45` calls
  `std::process::exit`, and the rest of the file contains CLI-only commands.
  A wasm binary has no process arguments or exit in the native sense.
- `crates/notwig-app` is currently only a binary crate. A browser build needs
  a `cdylib` target and an exported `wasm-bindgen` handle, or a small
  `notwig-web` crate that depends on the app library.

### 2.2 Rendering

- The app forces `eframe::Renderer::Wgpu` at `crates/notwig-app/src/gui.rs:21`.
  WebGPU is available in current Chromium-based browsers, but not universally.
  The web build should initially fall back to `eframe::Renderer::Glow`/WebGL
  when WebGPU is unavailable, or explicitly document the WebGPU requirement.
- `wgpu` in `Cargo.toml` is declared with `features = ["metal"]`. That is
  native-only configuration. The wasm build needs the `webgpu` backend to stay
  enabled (it is part of `wgpu`'s default features today, so keep it) and must
  not treat Metal as a requirement for all targets.
- `crates/notwig-app/src/gui.rs:1423` uses the native `mouse_position` crate to
  recover the pointer position. Browser input is available directly through
  `egui::Context`; this helper must be gated or replaced for wasm.

### 2.3 Audio

- `crates/notwig-app/src/audio.rs` is built entirely on CPAL. CPAL 0.18 has a
  browser backend, but it is only compiled when the `wasm-bindgen` feature is
  enabled. The workspace currently declares `cpal = "0.18"` with no features.
- `crates/notwig-app/src/audio.rs:184` enumerates CPAL hosts and
  `:225` resolves a driver by name. In the browser there is one virtual browser
  audio host, not CoreAudio/ALSA/WASAPI devices. The settings model needs a
  browser-specific representation.
- `crates/notwig-app/src/gui.rs:167` calls `restart_audio()` during startup.
  Browsers block audio until a user gesture. The web app must create or resume
  the `AudioContext` from a click/keypress, not during construction.
- The CPAL `audioworklet` backend requires atomics/shared-memory support and is
  more restrictive to deploy. The first web milestone should use the
  `wasm-bindgen` WebAudio backend, which does not require cross-origin
  isolation. Revisit AudioWorklet only if latency demands it.
- `std::time::Instant` is used in `crates/notwig-app/src/audio.rs:4` and
  `crates/notwig-app/src/gui.rs:3`. On `wasm32-unknown-unknown` the standard
  library clock is not a reliable browser clock. Use `web-time` or an
  equivalent browser clock behind a small alias.

### 2.4 Threads

The application currently assumes OS threads. `std::thread::spawn` is not
supported on `wasm32-unknown-unknown`, and `std::thread::Builder::spawn`
returns an unavailable error or panics depending on the path.

- Audio import worker: `crates/notwig-app/src/gui.rs:112`.
- Audio duration analysis worker: `crates/notwig-app/src/gui.rs:131`.
- File dialog worker: `crates/notwig-app/src/gui.rs:1103`.
- Engine build worker: `crates/notwig-app/src/engine_build.rs:149`.
- Plugin scanner process and thread: `crates/notwig-app/src/plugins.rs:34`
  and `crates/notwig-app/src/plugins.rs:36` (`std::process::Command`).
- Extension index, install, and network work:
  `crates/notwig-app/src/extensions.rs:206`, `:225`, `:243`.
- Engine runtime loader: `crates/notwig-engine/src/control.rs:240`.

The web replacement is not "spawn a thread anyway". CPU-heavy work has to move
to a Web Worker, `spawn_local`/async work, or an incremental main-thread job.
For the first milestone, engine builds and audio decoding can run
synchronously in small chunks on the main thread. A Web Worker should be added
only after the `PreparedRuntime` transfer problem is solved (it contains
non-serializable processor objects).

### 2.5 Filesystem and project bundles

`notwig-project` is the storage authority and is written against `std::fs` and
`std::path::Path`:

- `crates/notwig-project/src/bundle.rs:40` reads the manifest,
  `:129` writes a PID-based session lock, and `:152` lists autosaves with
  `std::fs::read_dir`.
- `crates/notwig-project/src/assets.rs:46` opens audio with
  `std::fs::File::open`, and `:279` reads imported files from disk.
- `crates/notwig-project/src/export.rs:197` and `:245` create output files.
- `crates/notwig-project/src/fsutil.rs` provides directory creation and atomic
  rename helpers that only make sense on a real filesystem.
- `crates/notwig-project/src/probe.rs` uses `File`, `Read`, `Seek`, and
  metadata from the filesystem.

`notwig-app` adds more direct filesystem assumptions:

- `crates/notwig-app/src/workspace/mod.rs:404` saves plugin state, pending
  assets, and the bundle through `std::fs` at `:420`-`:450`.
- Autosave uses `SystemTime` at `crates/notwig-app/src/workspace/mod.rs:480`.
- Asset loading and import use bundle paths:
  `crates/notwig-app/src/workspace/assets.rs:20` and `:40`.
- Plugin state is read from `bundle/state` at
  `crates/notwig-app/src/workspace/mod.rs:489`.
- Extension discovery, install, uninstall, and config all use paths:
  `crates/notwig-ext/src/discovery.rs`, `crates/notwig-ext/src/install.rs`,
  `crates/notwig-app/src/extensions.rs:399`.
- Config files use `$HOME`:
  `crates/notwig-app/src/audio_config.rs:161`,
  `crates/notwig-app/src/shortcuts.rs:33`, and
  `crates/notwig-app/src/plugins.rs:91`.

The browser does have persistent storage, but it is asynchronous and
capability-based (Origin Private File System, IndexedDB, Cache Storage, or the
File System Access API). It cannot be used through the current synchronous
`std::fs` API.

### 2.6 File dialogs and drag/drop

- Every native dialog is `rfd::FileDialog`:
  `crates/notwig-app/src/gui.rs:949` (open project),
  `:983`/`:996` (render destination), `:1066` (extension ZIP), `:1087`
  (save as), and `:1438` (audio import).
  The synchronous `FileDialog` API is not available on wasm. `rfd` does provide
  an asynchronous web path, but the app currently calls it from a worker thread
  and expects a `PathBuf`.
- Drag/drop is path-only. `crates/notwig-ui/src/views/shell.rs:1381` reads
  `egui::DroppedFile::path` and ignores `DroppedFile::bytes`. On the web,
  `path` is `None`; the file contents are in `bytes`, and `name` replaces the
  path. The pending drop queue in `crates/notwig-app/src/gui.rs:70` is
  `(PathBuf, DropAudioTarget, TimeMus)` and cannot represent a browser file.
- UI intents and runtime views carry `PathBuf` as a file identity:
  `crates/notwig-ui/src/intent.rs:211`, `:214`, `:267`,
  `crates/notwig-ui/src/runtime.rs:94`, and
  `crates/notwig-ui/src/widgets/drop_target.rs:12`.
  `PathBuf` itself is usable on wasm as an opaque string container, but these
  values must stop meaning "a path on the local filesystem."

### 2.7 Native plugins

- `notwig-plugins` defaults to `vst3` and `au` features
  (`crates/notwig-plugins/Cargo.toml`). Those pull in the excluded,
  platform-specific `notwig-truce-rack` host and, on macOS, `objc2` /
  `objc2-foundation`.
- `crates/notwig-plugins/src/adapters.rs:12` reads plugin binaries from disk.
- `crates/notwig-plugins/src/native_editor.rs` creates `NSWindow` and `NSView`
  handles on macOS.
- `crates/notwig-app/src/plugins.rs` launches the current executable as a
  plugin scanner at `:36`, which is impossible in a browser.

Native plugins cannot be made browser-compatible. The web build must compile
the plugin crate with no formats, present an empty plugin catalog, hide plugin
insertion, and still open projects that contain plugin devices without
crashing.

### 2.8 Extensions

- `notwig-ext` depends on `wasmtime` for JIT execution
  (`crates/notwig-ext/src/abi.rs:5`, `crates/notwig-ext/src/host.rs:10`).
  wasmtime's Cranelift JIT does not run on `wasm32-unknown-unknown`.
- Extension discovery reads directories and files with `std::fs`
  (`crates/notwig-ext/src/discovery.rs:33`, `:73`, `:94`, `:105`).
- Extension installation uses staging directories, renames, `std::process::id`,
  and `SystemTime` (`crates/notwig-ext/src/install.rs:40`, `:47`, `:63`, `:253`).
- The extension index download uses blocking `ureq` on a worker thread
  (`crates/notwig-app/src/extensions.rs:368`, `:382`).

The first milestone should disable extensions in the browser. The later
browser host uses the browser's `WebAssembly` API instead of wasmtime and the
same storage abstraction as projects. Fuel/interrupt limits are not available
through the browser API, so trust levels will be more limited than on native.

### 2.9 Network and configuration

- `crates/notwig-app/src/extensions.rs:382` uses blocking `ureq` from a
  thread. The browser needs `fetch` (`web-sys`) with the same size and timeout
  rules.
- User configuration is stored in files under `$HOME`. The browser needs
  `localStorage`/IndexedDB or an OPFS settings file.
- `tracing_subscriber::fmt` is used in `crates/notwig-app/src/logging.rs`.
  Browser logging should go to `console` through `tracing-wasm` or an
  equivalent layer, and the panic hook should be installed by the web runner.

### 2.10 Dependency/build configuration

| Dependency | Native use | Browser issue | Planned fix |
|---|---|---|---|
| `eframe` | `run_native`, `NativeOptions` | wasm needs `WebRunner`/`WebOptions` | Add a web entry point and feature gate the native entry |
| `wgpu` | forced native renderer | WebGPU/WebGL backend selection | Keep `webgpu` available, add Glow fallback |
| `cpal` | CoreAudio/ALSA/WASAPI | needs the `wasm-bindgen` feature | Enable browser backend and defer start until user gesture |
| `rfd` | synchronous `FileDialog` | web supports async dialogs only | Add an async dialog service |
| `mouse_position` | native pointer lookup | not meaningful in the browser | Gate native-only |
| `ureq` | blocking HTTP | synchronous blocking network | Use browser `fetch` on wasm |
| `wasmtime` | extension JIT | no Cranelift JIT on wasm | Move behind a native feature; web host later using `js-sys`/`web-sys` |
| `truce-rack` | VST3/AU | native ABI and GUI | Do not compile for wasm |
| `tracing-subscriber` | native logging | file/time assumptions, heavy | Use `tracing-wasm` on web |
| `uuid` | IDs | v4 needs browser randomness | Enable `uuid/js` on wasm |
| `zip`, `symphonia`, `hound`, `sha2` | bundle/assets | pure Rust, but called through file paths | Keep crates, switch our APIs to byte buffers |
| `flacenc`, `rusty_mp3`, `rusty_vorbis`, `ogg` | export/import | need per-crate wasm compile audit | Compile-check; feature-gate unsupported encoders if necessary |

The current CI already installs the `wasm32-unknown-unknown` target for the
stock extension build. That is useful, but the application has no wasm target,
no web crate, and no wasm build command.

## 3. Target architecture

```text
notwig-web (cdylib, wasm-bindgen)
  |
  |  exports a WebApp handle; starts eframe::WebRunner with a canvas
  v
notwig-app
  |
  |  shared NotwigApp: UI intents, workspace policy, history, runtime projection
  v
Platform (cfg(target_arch = "wasm32"))
  |
  +-- native: std::fs, std::thread, rfd sync dialogs, CPAL native,
  |           truce-rack, wasmtime, $HOME config
  |
  +-- web:    OPFS/IndexedDB, spawn_local/Web Worker, browser pickers,
              CPAL WebAudio, browser WebAssembly, localStorage config
```

Keep the shared layers unchanged where possible. The platform edges should be
traits or `cfg`-selected modules, not `#[cfg]` branches scattered through
domain logic.

### 3.1 Add a `notwig-web` crate

Create `crates/notwig-web`:

```toml
[package]
name = "notwig-web"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
notwig-app = { path = "../notwig-app", default-features = false, features = ["web"] }
eframe.workspace = true
wasm-bindgen = "0.2"
wasm-bindgen-futures = "0.4"
web-sys = { version = "0.3", features = ["HtmlCanvasElement", "Window", "Document"] }
js-sys = "0.3"
```

The crate exposes:

```rust
#[wasm_bindgen]
pub struct WebApp {
    runner: eframe::WebRunner,
}

#[wasm_bindgen]
impl WebApp {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self { /* install logger/panic hook */ }

    #[wasm_bindgen]
    pub async fn start(&self, canvas: web_sys::HtmlCanvasElement)
        -> Result<(), wasm_bindgen::JsValue>
    {
        self.runner
            .start(canvas, eframe::WebOptions::default(), Box::new(|cc| {
                Ok(Box::new(notwig_app::web::NotwigApp::new(cc)))
            }))
            .await
    }
}
```

`notwig-app` gets a `web` feature and a `web` module. The native binary keeps
using `gui::run`. `notwig-web` is the only crate built for the browser.

### 3.2 Storage abstraction

Introduce a byte-oriented bundle layer in `notwig-project` rather than writing
a second project format.

Proposed boundary:

```rust
pub struct BundleFiles {
    pub files: BTreeMap<String, Vec<u8>>,
}

pub trait BundleStore {
    fn list(&self) -> Result<Vec<BundleId>, ProjectError>;
    fn load(&self, id: &BundleId) -> Result<BundleFiles, ProjectError>;
    fn save(&self, id: &BundleId, files: &BundleFiles) -> Result<(), ProjectError>;
    fn remove(&self, id: &BundleId) -> Result<(), ProjectError>;
}
```

- `notwig-project` keeps `open`, `save`, `autosave`, and `load_latest_autosave`
  as native path wrappers implemented over the same file-map core.
- Add `open_files`, `save_files`, and `autosave_files` that operate on
  `BundleFiles` and are testable without a filesystem.
- Native `FsBundleStore` reads/writes the existing directory layout
  (`project.json`, `assets/`, `autosave/`, `state/`, `peaks/`, `cache/`).
- Web `OpfsBundleStore` keeps the same logical file map and persists it to the
  Origin Private File System. OPFS operations are asynchronous; run them through
  `wasm_bindgen_futures` and return results through a channel that the UI polls,
  or keep an in-memory map and write it asynchronously after each save.
- For the first milestone, loading a bundle into memory is acceptable. The app
  already holds all audio assets in memory in `Workspace::assets`, so the
  browser bundle can be a `BTreeMap<String, Vec<u8>>` with asynchronous
  persistence behind it.
- Import/export uses a zip container whose root mirrors the bundle:
  `project.json`, `assets/...`, `state/...`, `autosave/...`.
  The web picker reads a zip into memory; download writes the zip through a
  Blob URL. Native can keep using directories and add zip import/export later.

The project bundle identity in the UI should be a stable display name plus a
`BundleId`, not an OS path. `PathBuf` can remain as an opaque key internally
where that keeps the change small, but it must not be passed to `std::fs` on
wasm.

Session lock must become a `SessionLease` abstraction:

- Native: keep the current `session.lock` file and PID.
- Web: use a tab-scoped lease in `localStorage` or `navigator.locks`, or skip
  stale-lock detection and rely on autosave snapshots. There is no meaningful
  browser equivalent of `std::process::id()`.

### 3.3 Byte-oriented audio I/O

Change the project audio boundary from paths to named byte buffers:

```rust
pub struct AudioSource {
    pub name: String,
    pub bytes: Vec<u8>,
}

pub fn decode_audio_bytes(source: &AudioSource) -> Result<AudioAssetBuffer, ProjectError>;
pub fn probe_audio_bytes(source: &AudioSource) -> Result<AudioProbe, ProjectError>;
pub fn import_asset_bytes(source: AudioSource, project_rate: u32)
    -> Result<ImportedAsset, ProjectError>;
pub fn encode_audio_export(samples: &[f32], ..., options: ExportOptions)
    -> Result<Vec<u8>, ProjectError>;
```

- `decode_symphonia` uses `Cursor<Vec<u8>>` instead of `std::fs::File`.
- `probe_*` uses a cursor and the source name for the format hint.
- `write_wav_f32` keeps a native path wrapper and gains a byte-returning
  implementation for import and export.
- `write_audio_export` keeps the native path wrapper and gains
  `encode_audio_export` returning bytes. The web platform turns the bytes into
  a `Blob` and downloads it; native writes them to a path.
- Native callers read the source file once and pass bytes. This also improves
  testability and makes drag/drop work naturally.

Render stems should be packaged as a zip for download or written to OPFS.

### 3.4 Tasks and dialogs

Add a small platform service in `notwig-app`:

```rust
pub trait Tasks {
    fn spawn(&self, job: Box<dyn FnOnce() + Send + 'static>);
}

pub trait Dialogs {
    fn open_project(&self, reply: Sender<DialogResult>);
    fn save_project(&self, suggested_name: String, reply: Sender<DialogResult>);
    fn open_audio(&self, reply: Sender<DialogResult>);
    fn open_extension_zip(&self, reply: Sender<DialogResult>);
    fn choose_render_destination(&self, reply: Sender<DialogResult>);
}
```

- Native `Tasks` uses named OS threads.
- Web `Tasks` uses `spawn_local` for async work and a chunked job queue for
  CPU work. Engine build stays synchronous in the first milestone.
- Native `Dialogs` keeps the current `rfd::FileDialog` behavior in a worker
  thread.
- Web `Dialogs` uses `rfd::AsyncFileDialog` or a small `<input type="file">`
  bridge and returns `PickedFile { name, bytes }`. The app never blocks waiting
  for a browser dialog.
- Drag/drop becomes `PendingDrop { name, bytes, target, at }`. The UI's
  `handle_dropped_files` uses `DroppedFile::bytes` on wasm and `path` on
  native.
- Replace direct `audio_analysis_tx.send(PathBuf)` with a key that can refer to
  either a native path or a browser byte buffer. A simple `AssetSourceKey` enum
  (`Path(PathBuf)` / `Bytes { name, bytes }`) is enough.

### 3.5 Audio service

Split `notwig-app/src/audio.rs` into a shared snapshot/settings layer and
platform backends:

- `audio/mod.rs`: `AudioSettings`, `AudioSnapshot`, DSP history, settings view.
- `audio/native.rs`: current CPAL device enumeration and stream creation.
- `audio/web.rs`: one browser audio host, WebAudio stream through CPAL with
  the `wasm-bindgen` feature, no device enumeration, and an explicit
  `ensure_started()` that runs from a user gesture.

Web behavior:

- Show "Click to enable audio" until the first click/keypress.
- Create the `AudioContext` and start the CPAL stream then.
- Map the project sample rate to the context sample rate where possible; do
  not require a fixed device block size.
- Suspend on tab hide and resume on return.
- Never claim a native driver/device name. Display "Browser audio".

### 3.6 Plugins and extensions

`notwig-plugins`:

- Change `default = []`; keep `vst3` and `au` as opt-in features.
- Native `notwig-app` enables `vst3` and `au`.
- Web `notwig-app` enables neither.
- `PluginHost::new()` on wasm returns an empty host.
- `PluginCatalog::global()` returns an empty catalog without starting the
  scanner process.
- Hide plugin insertion in the browser UI and show a clear status when a loaded
  project contains plugin devices.
- Add a platform capability flag such as `supports_native_plugins` so the UI
  can disable the plugin browser without target-specific branches in views.
- The engine build should not fail the whole project when a plugin descriptor
  is missing. Prefer a bypass/silent device plus a status warning so the rest
  of the project opens.

`notwig-ext`:

- Feature-gate the wasmtime backend as `native`.
- Add a `web` backend in a later milestone using
  `js_sys::WebAssembly::{Module, Instance}` and host imports exposed through
  `wasm-bindgen` closures. The existing extension ABI can be reused because it
  is already memory/pointer based.
- In the first milestone, `ExtensionManager` reports "Extensions are not
  available in the browser" and returns no hosts, specs, panels, or index
  entries.
- Extension installation/discovery uses the same storage abstraction as
  projects, not `Path`.
- Browser fetch replaces `ureq`.
- Trust levels become validation limits in the browser; fuel interruption is
  not exposed by the WebAssembly JS API.

### 3.7 Logging, panics, and config

- Native: keep `tracing-subscriber`.
- Web: use `tracing-wasm` or a minimal `console` layer and
  `console_error_panic_hook` (eframe's web runner already installs a panic
  boundary; use it for the user-facing panic summary).
- Replace file-based `audio_config`, `shortcuts`, extension config, and plugin
  cache settings with a `SettingsStore` abstraction:
  - native: existing JSON files;
  - web: `localStorage` keys with the same versioned JSON values.
- Timestamp/autosave logic uses `web_time::SystemTime`/`Instant` on wasm via a
  small `notwig-app::clock` alias.

## 4. Implementation phases

### Phase 0: prove the build

- Add the `wasm32-unknown-unknown` target to local instructions and CI.
- Add `crates/notwig-web` with a blank eframe canvas.
- Add `web`/`native` features to `notwig-app` and gate `main.rs`, `gui::run`,
  `mouse_position`, `tracing-subscriber`, and native-only modules.
- Compile with no audio, no plugins, no extensions, and no workers.
- Exit criterion: `wasm-pack build crates/notwig-web --target web` succeeds and
  a browser renders the eframe canvas.

### Phase 1: storage and byte-based project I/O

- Add `BundleFiles`, `BundleStore`, `open_files`, `save_files`,
  `autosave_files`, and a native `FsBundleStore`.
- Add an in-memory store for tests and a temporary web store.
- Convert audio decode/probe/import/export to byte buffers.
- Add zip import/export for the browser and native.
- Remove `PathBuf` from app-level asset identity where it means a local file.
- Exit criterion: a project can be created, edited, saved, and reopened in a
  wasm test harness with no filesystem.

### Phase 2: platform services in the app

- Extract `NotwigApp` from `gui.rs` into a target-neutral module.
- Add `Tasks`, `Dialogs`, `SettingsStore`, and `clock`.
- Replace `std::thread::spawn` in the app with the task service.
- Replace `std::fs` calls in `workspace` with the bundle store.
- Replace `SystemTime`/`Instant` with the clock alias.
- Exit criterion: `NotwigApp::new` works on wasm with dialogs and tasks stubbed.

### Phase 3: audio

- Enable CPAL's `wasm-bindgen` feature.
- Add `audio/web.rs` with one browser audio host and gesture-gated startup.
- Adapt audio settings UI to show browser audio instead of driver/device lists.
- Verify transport, loop, meters, and stock-device playback.
- Exit criterion: the demo project plays in a browser after a user gesture.

### Phase 4: native-only features degrade cleanly

- Disable VST3/AU features on wasm and return an empty plugin catalog.
- Hide plugin insertion and show a status for unavailable plugin devices.
- Disable extension host initialization and show a clear browser message.
- Make loading a project with plugins/extensions succeed with warnings.
- Exit criterion: a native project with unsupported plugins opens and plays
  its stock-device and audio content.

### Phase 5: browser storage, pickers, and downloads

- Implement `OpfsBundleStore` and `localStorage` settings.
- Wire `rfd::AsyncFileDialog` or the file-input bridge.
- Wire dropped-file bytes.
- Wire render/download and project zip export.
- Implement autosave recovery for browser storage.
- Exit criterion: the first-milestone checklist in section 1 works end to end.

### Phase 6: extension host (later)

- Add the browser WebAssembly extension host.
- Reuse the existing ABI and draw IR.
- Move extension discovery/install onto the bundle storage abstraction.
- Add capability and trust reporting appropriate to the browser.
- Exit criterion: the stock limiter extension loads and processes audio in the
  browser.

## 5. Concrete file and crate changes

| Area | Files | Change |
|---|---|---|
| Web entry | new `crates/notwig-web/` | `cdylib`, `wasm-bindgen` handle, eframe `WebRunner` |
| App entry | `crates/notwig-app/src/main.rs`, `src/lib.rs`, `src/gui.rs` | Native CLI/run path gated; target-neutral app extraction |
| Platform services | new `crates/notwig-app/src/platform/` | `Tasks`, `Dialogs`, `SettingsStore`, `clock` |
| Audio | `crates/notwig-app/src/audio.rs` | Split native/web backends; gesture-gated browser audio |
| Project I/O | `crates/notwig-project/src/bundle.rs`, `assets.rs`, `export.rs`, `probe.rs`, `fsutil.rs` | Byte-oriented core plus native path wrappers |
| Bundle store | new `crates/notwig-project/src/store.rs` | `BundleFiles`, `BundleStore`, `FsBundleStore` |
| App workspace | `crates/notwig-app/src/workspace/mod.rs`, `workspace/assets.rs` | Use `BundleStore`; remove direct `std::fs` |
| UI intents | `crates/notwig-ui/src/intent.rs`, `runtime.rs`, `views/shell.rs`, `widgets/drop_target.rs` | Support byte-backed files and browser display names |
| Plugins | `crates/notwig-plugins/Cargo.toml`, `src/lib.rs`, `src/host.rs` | Default no formats; wasm empty host |
| Extensions | `crates/notwig-ext/Cargo.toml`, `src/lib.rs`, `src/abi.rs`, `src/host.rs` | Native wasmtime feature; web host later; storage abstraction |
| App extensions | `crates/notwig-app/src/extensions.rs` | Async fetch, storage-based install/discovery, web disable path |
| Logging | `crates/notwig-app/src/logging.rs` | `tracing-wasm` on wasm |
| Config | `crates/notwig-app/src/audio_config.rs`, `src/shortcuts.rs` | Settings store abstraction |
| Workspace | `Cargo.toml`, crate manifests | Features, optional native deps, wasm `uuid/js` |
| Web assets | new `web/index.html`, `web/styles.css` | Canvas page loading the wasm loader |
| CI | `.github/workflows/pages.yml`, `book.toml` | Build app into `book/app`, docs at domain root |

## 6. Proposed feature and dependency layout

```toml
# workspace
cpal = { version = "0.18", features = ["wasm-bindgen"] }
wgpu = { version = "25", features = ["webgpu"] }
```

```toml
# crates/notwig-core/Cargo.toml
[features]
default = ["rand"]
rand = ["uuid/v4"]
wasm = ["rand", "uuid/js"]
```

```toml
# crates/notwig-app/Cargo.toml
[features]
default = ["native"]
native = [
  "notwig-plugins/vst3",
  "notwig-plugins/au",
  "notwig-ext/native",
  "dep:mouse_position",
  "dep:tracing-subscriber",
  "dep:ureq",
]
web = [
  "notwig-core/wasm",
  "notwig-ext/web",
  "dep:wasm-bindgen",
  "dep:wasm-bindgen-futures",
  "dep:web-sys",
  "dep:js-sys",
]
```

```toml
# crates/notwig-plugins/Cargo.toml
[features]
default = []
vst3 = ["dep:truce-rack"]
au = ["dep:truce-rack"]
vst2 = []
```

```toml
# crates/notwig-ext/Cargo.toml
[features]
default = ["native"]
native = ["dep:wasmtime"]
web = ["dep:js-sys", "dep:web-sys", "dep:wasm-bindgen"]
```

Notes:

- Keep `wasmtime` as an optional dependency. It must not appear in the wasm
  dependency graph.
- `wgpu`'s default feature set includes `webgpu`; do not switch to
  `default-features = false` without re-adding it.
- `uuid`'s `js` feature uses WebCrypto, so it does not need the `getrandom`
  backend configuration in the browser.
- `notwig-dsp` already uses `notwig-core` with `default-features = false`.
  Check the feature unification after adding `notwig-core/wasm`.

## 7. CI and deployment

### 7.1 Domain and Pages prerequisites

The current site configuration is for a project subpath, not a custom domain:

- `.github/workflows/pages.yml` uploads `book` as the Pages artifact.
- `book.toml` sets `site-url = "/notwig/"`.
- There is no `CNAME` in the repository or in the uploaded artifact.

To serve docs at `https://notwig.lbirkert.com/` and the app at
`https://notwig.lbirkert.com/app/`:

1. In the GitHub repository settings, set Pages custom domain to
   `notwig.lbirkert.com` and enable HTTPS.
2. Add a DNS `CNAME` record from `notwig` to `lbirkert.github.io`.
3. Change `book.toml`:

   ```toml
   [output.html]
   site-url = "https://notwig.lbirkert.com/"
   ```

4. Add a `CNAME` file containing `notwig.lbirkert.com` to the Pages artifact
   root during CI, and add `.nojekyll`.
5. Keep the app under `book/app/`. The app's assets are relative to `/app/`,
   so it can be opened at `/app/` and `/app/index.html`.

If the docs are intentionally kept under a project path instead of a custom
domain, then the requested URL must become `/notwig/app/`; it cannot be
`/app/` from a project Pages site. This plan assumes the custom domain is set
up as requested.

### 7.2 Merged Pages workflow

Replace the current `build` job in `.github/workflows/pages.yml` with the
following shape. The extension packaging and mdbook steps are preserved; the
app build and Pages metadata are added before upload.

```yaml
name: Pages

on:
  push:
    branches: [main]
  workflow_dispatch:

permissions:
  contents: read
  pages: write
  id-token: write

concurrency:
  group: pages
  cancel-in-progress: true

env:
  CARGO_TERM_COLOR: always

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: wasm32-unknown-unknown

      - uses: Swatinem/rust-cache@v2

      - uses: actions/cache@v4
        id: extension-cache
        with:
          path: docs/extensions/notwig.ext.devices.zip
          key: notwig-ext-devices-${{ hashFiles('crates/notwig-ext-devices/**', 'crates/notwig-ext-api/**', 'crates/notwig-ext-macros/**', 'Cargo.lock') }}

      - name: Package stock extension
        if: steps.extension-cache.outputs.cache-hit != 'true'
        run: ./scripts/package-extension.sh

      - name: Build extension index
        run: |
          cp docs/extensions.txt /tmp/extensions.txt
          echo "docs/extensions/notwig.ext.devices.zip" >> /tmp/extensions.txt
          EXTENSIONS_LIST=/tmp/extensions.txt ./scripts/build-extensions.sh

      - uses: taiki-e/install-action@v2
        with:
          tool: mdbook

      - uses: taiki-e/install-action@v2
        with:
          tool: wasm-pack

      - name: Build docs
        run: mdbook build

      - name: Build web app
        run: |
          wasm-pack build crates/notwig-web \
            --target web \
            --release \
            --out-dir ../../book/app/pkg \
            --out-name notwig_web
          cp web/index.html book/app/index.html
          cp web/styles.css book/app/styles.css

      - name: Add extension assets
        run: |
          mkdir -p book/extensions
          cp docs/extensions.json book/extensions.json
          cp docs/extensions/*.zip book/extensions/

      - name: Add Pages metadata
        run: |
          printf 'notwig.lbirkert.com\n' > book/CNAME
          touch book/.nojekyll

      - uses: actions/upload-pages-artifact@v3
        with:
          path: book

  deploy:
    needs: build
    runs-on: ubuntu-latest
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    steps:
      - id: deployment
        uses: actions/deploy-pages@v4
```

If `wasm-pack` is not available through the install action in the runner
image, replace that step with:

```yaml
      - name: Install wasm-pack
        run: cargo install wasm-pack --locked
```

`wasm-pack` is preferable to a hand-written `wasm-bindgen` invocation in CI
because it produces a predictable `pkg/` layout with the JS loader and wasm
file together.

### 7.3 Web page files

Commit `web/index.html` and `web/styles.css`. The page must:

- create a full-window canvas with `id="notwig-canvas"`;
- import `./pkg/notwig_web.js`;
- call the generated `init()` before constructing `WebApp`;
- call `WebApp.start(canvas)`;
- show a "Click to enable audio" overlay until audio is allowed;
- keep all URLs relative so the app works at `/app/`.

Sketch:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Notwig</title>
    <link rel="stylesheet" href="./styles.css" />
  </head>
  <body>
    <canvas id="notwig-canvas"></canvas>
    <button id="enable-audio" type="button">Click to enable audio</button>
    <script type="module">
      import init, { WebApp } from "./pkg/notwig_web.js";

      await init();
      const canvas = document.getElementById("notwig-canvas");
      const app = new WebApp();
      await app.start(canvas);

      document.getElementById("enable-audio").addEventListener("click", async () => {
        document.getElementById("enable-audio").remove();
        await app.enable_audio();
      });
    </script>
  </body>
</html>
```

The exact `enable_audio` export is part of the `notwig-web` API and should call
the browser audio service's `ensure_started()`.

### 7.4 Local build and verification

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --locked

cargo check -p notwig-web --target wasm32-unknown-unknown
wasm-pack build crates/notwig-web --target web --release \
  --out-dir ../../target/web/pkg --out-name notwig_web

mkdir -p target/web
cp web/index.html web/styles.css target/web/
python3 -m http.server --directory target/web 8080
# open http://localhost:8080/
```

CI must run at least:

```sh
cargo check -p notwig-web --target wasm32-unknown-unknown
wasm-pack build crates/notwig-web --target web --release --out-dir /tmp/notwig-web --out-name notwig_web
```

Keep the native checks unchanged:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## 8. Testing strategy

- `notwig-project` unit/integration tests for `BundleFiles` and byte-based
  decode/probe/export using in-memory buffers. These tests must not touch
  `std::fs`.
- Store conformance tests shared by `FsBundleStore` and an in-memory store:
  create, save, reopen, autosave, list, remove, missing asset handling.
- Feature-gated compile tests for wasm. The main guard is
  `cargo check -p notwig-web --target wasm32-unknown-unknown`; add a CI job that
  fails if `wasmtime`, `truce-rack`, `objc2`, `std::process`, or native file
  dialog code leaks into the wasm graph.
- Browser smoke test: load `/app/`, wait for the canvas, check that the eframe
  app has not panicked, create a new project, and verify the transport state.
  Playwright/Chromium is the simplest option; make it optional until the web
  app is stable.
- Manual audio test: click to enable audio, play the demo project, verify
  meters and transport. CI cannot validate audible output reliably.
- Test loading a project that contains a native plugin device and confirm the
  web build opens it with a bypass/warning instead of failing.

## 9. Risks and trade-offs

- **Memory:** wasm32 linear memory is limited and audio assets are already
  loaded into memory. Large projects may need streaming or a worker-backed
  bundle store. Measure before optimizing.
- **Audio latency:** the WebAudio backend is higher latency than CoreAudio.
  AudioWorklet helps but adds deployment constraints (atomics, shared memory,
  headers). Start with the simpler backend.
- **WebGPU coverage:** WebGPU is not universal. Keep a WebGL/Glow fallback or
  state the requirement clearly. Test Safari and Firefox before announcing the
  URL.
- **Engine build on the UI thread:** synchronous builds will stutter on large
  projects. This is acceptable for the first milestone and should be measured
  with the demo and a larger fixture.
- **Plugin project compatibility:** projects with VST3/AU devices cannot sound
  the same in the browser. The app must show this clearly and preserve plugin
  state so the project remains editable and can be reopened natively.
- **Extension security:** browser WebAssembly cannot use wasmtime fuel. Do not
  claim the same sandboxing guarantees in the browser until the host enforces
  equivalent limits.
- **Custom domain:** the current `site-url` and missing `CNAME` mean the
  existing Pages workflow is not configured for `notwig.lbirkert.com`. That is
  a deployment prerequisite, not a code issue.

## 10. Definition of done

The browser milestone is done when:

- `cargo check -p notwig-web --target wasm32-unknown-unknown` passes on a clean
  checkout.
- The Pages workflow publishes the docs site at `/` and the app at `/app/`.
- The app starts without panicking in Chromium, Safari, and Firefox.
- A user can create a project, import WAV/AIFF audio, edit clips and MIDI, play
  back stock devices after enabling audio, save, reload, and render a mixdown.
- Native plugins and extensions are visibly disabled in the browser with a
  clear explanation, and projects containing them still open.
- Native macOS checks and tests still pass.
