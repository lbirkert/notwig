# Package and Manifest

## Directory layout

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

`extension.json` declares the package and its devices:

```json
{
  "id": "com.example.gain",
  "name": "Example Gain",
  "vendor": "Example",
  "version": "0.1.0",
  "api_version": 3,
  "entrypoints": [
    {
      "entrypoint": "my_gain.dylib",
      "target": "macos-aarch64"
    },
    {
      "entrypoint": "my_gain.wasm",
      "target": "wasm"
    }
  ],
  "capabilities": ["device", "ui.egui"],
  "devices": [
    {
      "name": "Example Gain",
      "type_id": "com.example.gain.fx"
    }
  ],
  "description": "A minimal gain device.",
  "image": "icon.png"
}
```

| Field | Required | Meaning |
| --- | --- | --- |
| `id` | yes | Stable package identity. Device type IDs should use it as a prefix. |
| `name` | yes | Display name in the extension explorer. |
| `vendor` | no | Author or vendor shown in the explorer. |
| `version` | yes | Package version string. |
| `api_version` | yes | Must match `notwig_ext_api::ABI_VERSION`, currently `3`. |
| `entrypoints` | yes | Ordered target builds. The first supported entry wins. |
| `capabilities` | no | Host services the package needs. |
| `devices` | yes | Devices registered by the package. |
| `description` | no | Short text shown in the explorer. |
| `image` | no | Relative icon path shown in the explorer. |

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
| `ui.egui` | Guest-owned egui panels. |

An extension without `ui.egui` still appears in the device browser and uses the
generic parameter view. See the
[development design page](../03_development/12_egui_extension_panels.md).

### Entrypoints and targets

Each entrypoint names a file relative to the package directory and the target it
was built for:

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
are rejected; every manifest must declare an `entrypoints` list.

### Native and WASM

WASM extensions run in a sandboxed runtime with a memory limit and per-call fuel
metering.

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

## Packaging from a workspace

The repository includes `scripts/package-extension.sh`. It builds
`notwig-ext-devices` for one target, substitutes the `$$TARGET` and
`$$ENTRYPOINT` placeholders in the manifest template, and writes a ZIP containing
`extension.json`, the target binary, and the icon:

```sh
./scripts/package-extension.sh wasm
./scripts/package-extension.sh macos-aarch64
```

Supported targets are `wasm`, `macos-aarch64`, `macos-x86_64`, `linux-x86_64`,
`linux-aarch64`, `windows-x86_64`, and `windows-aarch64`. Adding a target is a
case in the script. With no argument the script builds WASM, plus the macOS
Apple Silicon build when running on an Apple Silicon Mac.

Use the script as a template for your own release script; see
[Testing and Distribution](05_testing_and_distribution.md).
