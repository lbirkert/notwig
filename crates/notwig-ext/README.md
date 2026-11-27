# notwig-ext

The extension subsystem for Notwig. This directory contains the author SDK,
proc macro, host runtime, example devices, and the packaging/scaffolding CLI.

## Crates

| Crate | Purpose |
| --- | --- |
| [`notwig-ext-api`](notwig-ext-api) | Author-facing SDK, device/command/audio sub-editor contracts, guest panel transport, note events, ABI helpers |
| [`notwig-ext-macros`](notwig-ext-macros) | `#[notwig_ext(load)]` proc macro that generates WASM and native exports |
| [`notwig-ext`](notwig-ext) | Discovery, WASM/native loading, DSP instances, frame validation, and panel host |
| [`notwig-ext-devices`](notwig-ext-devices) | Bundled compressor, clipper, delay, chorus, transient, dynamics, reverb, synth, and gain devices |
| [`notwig-ext-example`](notwig-ext-example) | Command and custom audio sub-editor example |
| [`cargo-notwig-ext`](cargo-notwig-ext) | `cargo notwig-ext new`, `init`, `build`, and `targets` commands for extension authors |

## How an extension is shaped

A source extension is a normal Cargo library crate with an `extension.json`
manifest and an optional `icon.png`. The manifest omits `entrypoints`; the CLI
builds or accepts one target artifact at a time, adds the matching entrypoint,
and writes the installed ZIP layout:

```text
my-extension/
  Cargo.toml
  extension.json       # source: no entrypoints; optional includes
  icon.png             # included by default
  src/lib.rs           # #[notwig_ext(load)] entrypoint (optional preload/unload phases)
  assets/...           # extra files bundled through `includes`
```

```text
my-extension.zip
  extension.json       # transformed: includes removed, entrypoints added
  icon.png
  my_extension.wasm    # or lib*.dylib/.so, *.dll
  assets/...           # matching includes
```

The installed host accepts only the transformed package. Installed manifests
must declare at least one entrypoint; source manifests may omit them. The
current ABI version is `7`.

### Source manifest fields

Source manifests use the same fields as installed manifests, plus `includes`.
`entrypoints` may be omitted in source manifests.

```json
{
  "id": "com.example.my-extension",
  "name": "My Extension",
  "vendor": "Example",
  "version": "0.1.0",
  "api_version": 7,
  "capabilities": ["device", "ui.panel"],
  "devices": [
    { "name": "My Device", "type_id": "com.example.my-extension" }
  ],
  "description": "A new Notwig extension.",
  "image": "icon.png",
  "includes": [
    { "path": "assets/impulse.wav" },
    { "path": "assets/wasm-only.bin", "target": "wasm" }
  ]
}
```

`includes` entries name files relative to the extension root. An optional
`target` gates the file to one build target. The CLI removes `includes` from the
packaged manifest. `extension.json` and `icon.png` are always packaged when
present; the `image` path is packaged in addition when it differs from
`icon.png`.

## Targets

| Target | Rust triple | Entrypoint |
| --- | --- | --- |
| `wasm` | `wasm32-unknown-unknown` | `.wasm` |
| `macos-aarch64` | `aarch64-apple-darwin` | `.dylib` |
| `macos-x86_64` | `x86_64-apple-darwin` | `.dylib` |
| `linux-x86_64` | `x86_64-unknown-linux-gnu` | `.so` |
| `linux-aarch64` | `aarch64-unknown-linux-gnu` | `.so` |
| `windows-x86_64` | `x86_64-pc-windows-msvc` | `.dll` |
| `windows-aarch64` | `aarch64-pc-windows-msvc` | `.dll` |

`wasm` is portable and works in the native and browser hosts. Native targets
run in-process and are not sandboxed. Non-host native targets build through
the target platform. Build each binary in a matching VM or machine, then copy
the `target/<rust-triple>/release/` artifact into one target directory and run
`cargo notwig-ext build --package` to bundle them together.

## CLI

```sh
# Create a new extension project.
cargo notwig-ext new my-extension

# Add extension files to an existing crate/package.
cargo notwig-ext init

# Build one target binary (run this in each target VM).
cargo notwig-ext build --entrypoint wasm
cargo notwig-ext build --entrypoint macos-aarch64

# After copying target artifacts together, package what is available.
cargo notwig-ext build --package --out-dir dist

# Package only selected targets and override the ZIP name.
cargo notwig-ext build --package macos-aarch64 wasm --out-dir docs/extensions --name ext-devices
```

`new` and `init` generate `Cargo.toml`, `src/lib.rs`, `extension.json`, and a
default `icon.png`. The generated device is a working gain effect using the
shared `notwig-ui` egui components.

## Host behavior

- **WASM** modules run under `wasmtime` on native and the browser `WebAssembly`
  API in the browser build. Native wasmtime enforces memory and fuel limits;
  the browser host relies on the browser sandbox and adds no explicit caps.
- **Native** shared libraries load into the Notwig process and have the same
  access as the application. Only install native extensions from trusted
  sources.
- **Panels** use the guest egui API. Extensions run a private egui context and
  return validated meshes, texture deltas, and parameter actions. The host
  composites the output and applies validated actions. Audio sub-editors use
  the same frame transport with a bounded clip context and host-owned edits.
- **Commands** use the same package and macro entrypoint. Manifest version 2
  contributions declare command metadata; the host applies validated edit
  families and one undo entry. See
  [`docs/02_extensions/06_command_api.md`](../../docs/02_extensions/06_command_api.md).
- **Packaging** is produced by `cargo notwig-ext`. The Pages workflow shows a
  minimal CI recipe: install the CLI from this workspace, build one target
  entrypoint on the matching runner, then package the artifacts into a ZIP.

See [`docs/02_extensions/`](../../docs/02_extensions/) for author documentation
and [`docs/03_development/`](../../docs/03_development/) for the runtime design.
