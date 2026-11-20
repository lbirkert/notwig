# Package and Manifest

## Directory layout

An installed extension is a directory under the per-user extension root:

```text
~/Library/Application Support/notwig/extensions/com.example.gain/
  extension.json
  my_gain.wasm
  icon.png            # optional
```

On platforms that use the XDG layout, the root is `~/.config/notwig/extensions`.
The directory name does not have to match the manifest `id`, but keeping them
equal avoids confusion.

## Manifest

`extension.json` declares the package and its devices:

```json
{
  "id": "com.example.gain",
  "name": "Example Gain",
  "vendor": "Example",
  "version": "0.1.0",
  "api_version": 2,
  "entry": "example_gain.wasm",
  "capabilities": ["device", "ui.draw"],
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
| `api_version` | yes | Must match `notwig_ext_api::ABI_VERSION`, currently `2`. |
| `entry` | no | Relative wasm path. Defaults to `extension.wasm`. |
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
| `ui.draw` | Custom panel rendering. |

An extension without `ui.draw` still appears in the device browser and uses the
generic parameter view. A `DrawOnly` trust level without `device` cannot process
audio.

### Entry module

`entry` is a path relative to the package directory. It must stay inside the
package. The module must be at most 8 MiB. The host does not follow absolute
paths or paths that escape the extension directory.

## Installing

### From a ZIP

The extension explorer accepts a ZIP with one of two layouts:

```text
extension.json          extension.json
example.wasm            example.wasm
icon.png
```

or:

```text
com.example.gain/
  extension.json
  example.wasm
  icon.png
```

The ZIP must contain exactly one `extension.json`, either at the root or inside
a single top-level directory. Unsafe paths, oversized archives, and manifests
whose IDs do not match the installed package are rejected.

Install from **Settings > Extensions > Add .ZIP**, or drop a `.zip` file onto
the Extensions page. Reinstalling an installed extension asks for confirmation
and replaces the previous files.

### Manual install

Create the directory under the extension root and copy the manifest, module,
and optional icon. Unlike a ZIP install, a manual copy does not reload the host.
Restart Notwig after copying.

### Uninstall

Use **Uninstall** in the extension explorer. It removes the package files and
the extension's settings, including trust level and enabled state. Projects that
use the device keep the device entry, but the device is unavailable until the
extension is installed again.

## Packaging from a workspace

The repository includes `scripts/package-extension.sh`. It builds
`notwig-ext-devices` for `wasm32-unknown-unknown` and writes a ZIP containing
`extension.json`, the wasm module, and the icon. Use it as a template for your
own release script; see
[Testing and Distribution](05_testing_and_distribution.md).
