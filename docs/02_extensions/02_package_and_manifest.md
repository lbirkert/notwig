# Package and Manifest

## Directory layout

An extension source project is a Cargo package with a manifest and an icon:

```text
my-gain/
  Cargo.toml
  extension.json       # source manifest: no entrypoints
  icon.png             # packaged by default
  src/lib.rs
  assets/...           # optional; listed in includes
```

An installed extension is a directory under the per-user extension root:

```text
~/Library/Application Support/notwig/extensions/com.example.gain/
  extension.json
  my_gain.dylib        # macOS shared library, or
  my_gain.wasm         # portable WebAssembly build
  icon.png             # optional
```

On platforms that use the XDG layout, the root is `~/.config/notwig/extensions`.
The directory name does not have to match the manifest `id`, but keeping them
equal avoids confusion.

A package can contain several target builds. The first entrypoint in manifest
order that the host can run is used.

## Manifest

The source `extension.json` declares the package and its devices. It omits
`entrypoints`; `cargo notwig-ext` adds the selected target entrypoint when it
builds a ZIP:

```json
{
  "manifest_version": 2,
  "id": "com.example.gain",
  "name": "Example Gain",
  "vendor": "Example",
  "version": "0.1.0",
  "api_version": 8,
  "capabilities": ["device", "ui.panel"],
  "devices": [
    {
      "name": "Example Gain",
      "type_id": "com.example.gain.fx"
    }
  ],
  "description": "A minimal gain device.",
  "image": "icon.png",
  "includes": [
    { "path": "assets/impulse.wav" },
    { "path": "assets/desktop-only.bin", "target": "macos-aarch64" }
  ]
}
```

The packaged manifest has `includes` removed and an `entrypoints` list added. A
ZIP may contain one or several target binaries; `--package` includes every
binary it finds in Cargo's target directory, or only the targets you list.

| Field | Required | Meaning |
| --- | --- | --- |
| `manifest_version` | no | Manifest layout. Omit or use `1` for device-only packages; contributions require `2`. |
| `id` | yes | Stable package identity. Device type IDs should use it as a prefix. |
| `name` | yes | Display name in the extension explorer. |
| `vendor` | no | Author or vendor shown in the explorer. |
| `version` | yes | Package version string. |
| `api_version` | yes | Must match `notwig_ext_api::ABI_VERSION`, currently `8`. |
| `entrypoints` | installed only | Ordered target builds; the CLI adds it. The first supported entry wins. |
| `includes` | source only | Extra files to bundle, optionally gated by target. The CLI removes it. |
| `capabilities` | no | Host services the package needs. |
| `devices` | no | Devices registered by the package. |
| `contributes` | no | Compatibility fallback for extension-point contributions. New packages register command and audio-editor metadata on the `notwig.commands` and `notwig.audio_editor.sub_editor` extension points at runtime. See the [Command API](06_command_api.md) and [Audio Sub-editor API](07_audio_sub_editor_api.md). |
| `description` | no | Short text shown in the explorer. |
| `image` | no | Relative icon path in the package. The index copies it to `extensions/<id>.png` for the explorer. |

### Identity rules

- `id` is the package identity. Keep it stable across releases.
- Each `devices[].type_id` must equal the `DeviceSpec::type_id` returned by the
  device.
- Two devices in one package must not share a type ID.
- Type IDs should be namespaced with the package ID, for example
  `com.example.gain.fx`.
- The `renderer` string in `DeviceUiSpec::custom` should use the device type ID,
  for example `ext:com.example.gain.fx`.

### Capabilities

| Capability | Grants |
| --- | --- |
| `device` | Device registration and DSP. |
| `ui.panel` | Guest-owned panels. |
| `command` | Command contributions and command palette entries. |
| `audio_editor` | Audio-editor sub-editor contributions. |

An extension without `ui.panel` still appears in the device browser and uses the
generic parameter view. See the
[development design page](../03_development/12_extension_panels.md).

### Includes

`includes` lists extra files to copy into the ZIP:

```json
"includes": [
  { "path": "assets/impulse.wav" },
  { "path": "assets/wasm-only.bin", "target": "wasm" }
]
```

- `path` is relative to the extension source root and must stay inside it.
- `target` is optional. When present, the file is included only for that build
  target. Target aliases such as `macos-arm64` are accepted.
- `extension.json` and `icon.png` are always included when present.
- The `image` path is included as well when it differs from `icon.png`.
- The CLI removes `includes` from the packaged manifest.

### Entrypoints and targets

`cargo notwig-ext build --package` adds one entrypoint per packaged binary:

```json
"entrypoints": [
  { "entrypoint": "my_gain.wasm", "target": "wasm" }
]
```

Each entrypoint names a file relative to the package directory and the target it
was built for. A multi-platform package looks like this:

```json
"entrypoints": [
  { "entrypoint": "notwig_ext_devices.dylib", "target": "macos-aarch64" },
  { "entrypoint": "notwig_ext_devices.wasm", "target": "wasm" }
]
```

The host walks the list from the start, selects the first target it can run, and
uses that file. List native builds before portable WASM builds so desktop users
get the native path. Put `wasm` last because it is supported everywhere.

Recognized canonical targets:

| Target | Runtime |
| --- | --- |
| `macos-aarch64` | Native shared library on macOS Apple Silicon. |
| `macos-x86_64` | Native shared library on macOS Intel. |
| `linux-x86_64` | Native shared library on Linux x86-64. |
| `linux-aarch64` | Native shared library on Linux ARM64. |
| `windows-x86_64` | Native DLL on Windows x86-64. |
| `windows-aarch64` | Native DLL on Windows ARM64. |
| `wasm` | WebAssembly through `wasmtime` or the browser API. |

Aliases are accepted, including `macos-arm64`, `linux-x64_86`, and Rust target
triples such as `aarch64-apple-darwin` and `wasm32-unknown-unknown`.

`entrypoint` is a path relative to the package directory. It must stay inside
the package. The host does not follow absolute paths or paths that escape the
extension directory. The older singular `entry` and `entrypoint` string fields
are rejected. Installed manifests must declare an `entrypoints` list; source
manifests omit it and `cargo notwig-ext` adds it.

### Native and WASM

WASM extensions run in a sandboxed runtime. Under the native host, wasmtime
enforces a 64 MiB memory limit and 20 million fuel per call. The browser host
uses the platform `WebAssembly` API and does not add a memory cap or fuel
metering; it relies on the browser sandbox.

Native shared libraries load into the Notwig process and have the same access as
the application. There is no sandbox for native code. Only install native
extensions from sources you trust. Native DSP processes audio in place without
copying through a WASM memory boundary.

The same `DeviceExtension` implementation works for both targets; only the
generated ABI wrappers differ.

## Installing

### From a ZIP

The extension explorer accepts a ZIP with one of two layouts:

```text
extension.json          extension.json
example.dylib           com.example.gain/
icon.png                  extension.json
                          example.dylib
                          icon.png
```

The ZIP must contain exactly one `extension.json`, either at the root or inside
a single top-level directory. Unsafe paths, oversized archives, and manifests
whose IDs do not match the installed package are rejected.

Install from **Settings > Extensions > Add .ZIP**, or drop a `.zip` file onto
the Extensions page. Reinstalling an installed extension asks for confirmation
and replaces the previous files.

### Per-target and fat ZIPs

A ZIP can hold one target build or several. Per-target ZIPs keep downloads small
and are what the stock extension builds by default. The index builder merges
several target ZIPs for the same extension into one explorer entry. A fat ZIP
lists several `entrypoints` in one manifest and works without any index-side
merging.

### Manual install

Create the directory under the extension root and copy the manifest, entrypoint
files, and optional icon. Unlike a ZIP install, a manual copy does not reload the
host. Restart Notwig after copying.

### Uninstall

Use **Uninstall** in the extension explorer. It removes the package files and
the extension's settings, including enabled state. Projects that use the device
keep the device entry, but the device is unavailable until the extension is
installed again.

## Packaging with the CLI

The `cargo-notwig-ext` crate separates building from packaging:

```sh
# Build one binary on each target machine or VM.
cargo notwig-ext build --entrypoint wasm
cargo notwig-ext build --entrypoint macos-aarch64

# Copy the target/<rust-triple>/release/ artifacts into one target directory.
cargo notwig-ext build --package --out-dir docs/extensions
```

`--package` bundles every target binary it finds, or only the targets passed as
arguments. Without `--name`, the ZIP is named `<extension-id>.zip` when it
contains several entrypoints and `<extension-id>-<target>.zip` for one. There is
no automatic native cross-compilation; build each binary on the target platform.

The Pages workflow is a complete build recipe: it installs the CLI from this
workspace, builds one entrypoint on the matching runner, then packages it. The
CLI also provides `new` and `init` to scaffold a working extension with
`Cargo.toml`, `src/lib.rs`, `extension.json`, and a default `icon.png`. See
[Testing and Distribution](05_testing_and_distribution.md).
