# Web build and storage

Status: browser builds cover project editing, OPFS storage, extension packages,
WebAudio output, and audio-file import. They have no autosave, recovery, native
plugins, or project-archive import.

Related: [Architecture](00_architecture.md), [Code Structure](01_code_structure.md),
[Persistence and recovery](14_persistence_and_recovery.md), and
[Testing and Distribution](../02_extensions/05_testing_and_distribution.md).

## Build and output layout

The local recipe builds the wasm package and serves a static directory:

```sh
rustup target add wasm32-unknown-unknown && cargo install wasm-pack --locked
wasm-pack build crates/notwig-web --target web --release \
  --out-dir ../../target/web/pkg --out-name notwig_web
cp web/index.html web/styles.css target/web/
python3 -m http.server --directory target/web 8080
```

`wasm-pack --target web` produces JS glue plus the wasm module; `web/index.html`
imports `./pkg/notwig_web.js`. CI publishes the mdBook output at the site root
and copies the same app files under `/app/`:

```text
book/
  app/{index.html, styles.css, pkg/notwig_web.js, pkg/notwig_web_bg.wasm}
  extensions.json
  extensions/{ext-devices-*.zip, *.png}
  CNAME, .nojekyll
  index.html, 01_users/, 02_extensions/, 03_development/   mdBook docs
```

The workflow (`.github/workflows/pages.yml`) builds extension ZIPs for wasm,
runs `scripts/build-extensions.sh`, `mdbook build`, and `wasm-pack`, copies
`docs/extensions.json` and extension assets, writes the CNAME, and deploys.
macOS extension builds are temporarily disabled to keep CI runtime down; the
matrix entry and the macOS rows in `docs/extensions.csv` are kept commented so
they can be restored together.

## eframe and WebRunner entry

`notwig-web` is a `cdylib` exporting `WebApp`. Its constructor initializes
`eframe::WebLogger`; `start(canvas)` calls `eframe::WebRunner::start` with
`WebOptions::default()` and `notwig_app::web::create_app`, which initializes the
tracing subscriber and builds `NotwigApp`. The browser subscriber sends `info`
and above to the console while buffering `debug` and above for OPFS.
`web/index.html` awaits wasm init, constructs `WebApp`, starts it on
`notwig-canvas`, and reveals the audio button. `WebApp` also exposes
`enable_audio`, `has_panicked`, and `panic_message`; the page installs drag
helpers read by `web::take_drag_pointer`.

## Deployment assumptions

The output is static and needs no server headers. The app is assumed to live
under `/app/` beside the docs. `extensions/web.rs::site_base_url` strips a
trailing slash, `/index.html`, and `/app`, then uses that origin and path for
`extensions.json` and the stock package; moving the app requires keeping those
assets at the computed base.

## OPFS layout and path rules

Browser projects use the Origin Private File System. `navigator.storage.getDirectory()`
is the root; the app creates and lists a `Projects` folder under it:

```text
OPFS root
  Projects/
    demo/{project.json, assets/<asset>.wav, state/<device>.json}
  notwig/
    logs/app-<timestamp>.log
  notwig-extensions/
    <extension-id>/{manifest.json, ...package files}
```

Paths are slash-separated relative strings. Read and write helpers split on `/`,
ignore empty segments, and create intermediate directories on write. Browser
logs are appended through a keep-existing-data writable stream; the frame loop
flushes the bounded in-memory log buffer into `notwig/logs` with an async local
task. See [`Logging and Diagnostics`](22_logging.md).

Project names replace `/`, `\`, NUL, and control characters with `-`, trim spaces
and dots, and fall back to `Untitled`; no `.notwig` suffix is added. Opened projects
are mounted as `browser://<name>`. Listing `Projects` skips dot-prefixed entries
and returns sorted directory names.

## Picker, download, and upload

Open and Save As use an in-app picker that lists `Projects` directories. Open
reads `project.json` and its manifest asset URIs. Save Project rewrites the
current directory; Save As deletes an existing target, recreates it, and writes
again, asking for confirmation on overwrite. Download reads every project file
recursively and emits `<name>.notwig.zip` with stored (uncompressed) entries.
There is no project ZIP upload or import; upload-like inputs are only the file
input for audio and extension ZIPs, so project exchange is download-only.

## Extension packages and discovery

Installed packages live at `notwig-extensions/<id>/`. On startup the manager
lists and loads those directories and migrates legacy loose `.zip` files into
directories before removing the ZIP. It also fetches the stock package from
`<site-base>/extensions/ext-devices-wasm.zip` unless that ID is marked
uninstalled. Only `wasm` entrypoints are eligible; native hosts are unavailable.
Index installs fetch the selected URL, verify SHA-256, and extract the archive to
OPFS; ZIP installs hash the input bytes and record the hash in config. Uninstall
removes the directory and records the ID in local storage; discovery and
registration skip uninstalled IDs.

## Local storage keys

Browser settings use `localStorage`; there is no browser project registry like
the native `projects.json`:

| Key | Contents |
|---|---|
| `notwig.recent-projects` | JSON array of up to 10 recently opened or saved project names |
| `notwig.extension-config` | Extension config: enable flags, installed hashes, index URLs |
| `notwig.uninstalled-extensions` | IDs suppressed during discovery and registration |
| `notwig.audio` | Persisted audio driver/device/block-size settings |
| `notwig.shortcuts` | Persisted keyboard shortcut map |

## Fetch and index behavior

`fetch_bytes` uses `window.fetch` with `cache: no-store`, so rebuilt indexes,
icons, and packages are not served from the browser HTTP cache. This avoids a
stale expected SHA-256 after an extension is republished. Non-OK is not found.
Each index source is capped at 4 MiB; documents merge by extension ID; relative
download/image URLs resolve against the index URL; icons over 4 MiB are skipped.
The default `https://notwig.lbirkert.com/extensions.json` is rewritten to the
site base when still configured; results stay in memory with no persistent
cache.

## WebAudio gesture gate

`NotwigApp::new_web` builds the workspace without audio. The page shows a
"Click to enable audio" button; its click calls `WebApp::enable_audio`, which
refreshes options and restarts audio through CPAL's WebAudio host. It does not
request microphone access. Microphones use `getUserMedia` only when an armed
track first opens its input; input accepts only `WebAudio`, so output and
recording need a gesture. Native VST3/VST2/CLAP/LV2/AU is not compiled on wasm;
the plugin catalog is empty, leaving stock and WebAssembly extensions.

## Browser limitations

- No autosave, `session.lock`, recovery modal, or multi-tab write protection.
- No atomic write/rename. Save As stages the complete bundle in a hidden directory, commits the manifest last, and only then replaces the target; a failed staging step leaves the old project intact. Stale files from an older revision are not removed.
- `read_bundle` reads only `project.json` and asset URIs, not `state/`; the wasm plugin-state loader is a no-op, so saved state blobs are not restored.
- No project archive import; only download, audio import, and extension ZIP install are available.
- Download ZIPs are uncompressed, so large projects produce large files.
- OPFS durability follows browser policy; the app never calls `navigator.storage.persist()`.
- The engine build and extension loading run on the UI thread in this milestone.
