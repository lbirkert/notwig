# notwig-ext

The extension subsystem for Notwig. This directory contains the author SDK,
proc macro, host runtime, example devices, and the packaging/scaffolding CLI.

## Crates

| Crate | Purpose |
| --- | --- |
| [`notwig-ext-api`](notwig-ext-api) | Author-facing SDK, device manifest, guest panel transport, note events, ABI helpers |
| [`notwig-ext-macros`](notwig-ext-macros) | `#[notwig_extension]` proc macro that generates WASM and native exports |
| [`notwig-ext`](notwig-ext) | Discovery, WASM/native loading, DSP instances, frame validation, and panel host |
| [`notwig-ext-devices`](notwig-ext-devices) | Reference limiter, gate, synth, and gain devices |
| [`notwig-ext-cli`](notwig-ext-cli) | `new`, `init`, `build`, and `targets` commands for extension authors |

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
  src/lib.rs           # #[notwig_extension] entrypoint
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
current ABI version is `4`.

### Source manifest fields

Source manifests use the same fields as installed manifests, plus `includes`.
`entrypoints` may be omitted in source manifests.

```json
{
  "id": "com.example.my-extension",
  "name": "My Extension",
  "vendor": "Example",
  "version": "0.1.0",
  "api_version": 4,
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
run in-process and are not sandboxed.

## CLI

```sh
# Create a new extension project.
notwig-ext-cli new my-extension

# Add extension files to an existing crate/package.
notwig-ext-cli init

# Build one target and write dist/<id>-<target>.zip.
notwig-ext-cli build wasm

# Override the ZIP location/name.
notwig-ext-cli build macos-aarch64 --out-dir docs/extensions --name ext-devices-macos-arm64
```

`new` and `init` generate `Cargo.toml`, `src/lib.rs`, `extension.json`, and a
default `icon.png`. The generated device is a working gain effect using the
shared `notwig-ui` egui components.

## Host behavior

- **WASM** modules run under `wasmtime` on native and the browser `WebAssembly`
  API in the browser build. They are sandboxed with memory/fuel limits.
- **Native** shared libraries load into the Notwig process and have the same
  access as the application. Only install native extensions from trusted
  sources.
- **Panels** use the guest egui API. Extensions run a private egui context and
  return validated meshes, texture deltas, and parameter actions. The host
  composites the output and applies validated actions.
- **Packaging** is produced by `notwig-ext-cli`. The repository's
  `scripts/package-extension.sh` and the Pages workflow use the CLI to build
  the stock extension.

See [`docs/02_extensions/`](../../docs/02_extensions/) for author documentation
and [`docs/03_development/`](../../docs/03_development/) for the runtime design.
