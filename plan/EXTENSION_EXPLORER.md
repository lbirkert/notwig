# Extension explorer

Status: first pass implemented.

## Goal

Turn Settings > Extensions into an extension manager:

- install from one or more index files (URL or local path);
- install from a local ZIP (file picker or drag/drop);
- enable, disable, and uninstall installed extensions;
- keep the existing per-extension trust levels;
- publish the official index from `docs/extensions.json` via GitHub Pages.

The interaction model is close to the Arduino Library Manager: the user adds
index sources, browses the entries, and installs a package. A package is a ZIP
whose root contains `extension.json` plus the wasm module and any assets.

## Index format

`docs/extensions.json` is a JSON array:

```json
[
  {
    "id": "notwig.ext.devices",
    "name": "Notwig Stock Devices",
    "vendor": "Notwig",
    "version": "0.1.0",
    "api_version": 1,
    "entrypoint": "notwig_ext_devices.wasm",
    "capabilities": ["device", "ui.draw"],
    "devices": [
      { "name": "Notwig Stock Limiter", "type_id": "notwig.ext.devices.limiter" }
    ],
    "url": "extensions/notwig.ext.devices.zip"
  }
]
```

Rules:

- `url` is added by the build script. It may be absolute or relative to the
  index source; the app resolves relative URLs against the index location.
- Extension manifests accept both `entry` and `entrypoint`; the index script
  emits `entrypoint`. Device declarations may include a display `name`, which
  the index script carries through.
- Entries are merged across index sources by `id`. The highest valid semver
  wins; if a version is not semver, the later entry wins.
- The index entry is metadata only. Installation always trusts the
  `extension.json` inside the downloaded ZIP, validates it, and requires its
  `id` to match the requested entry.

Types live in `notwig-ext-api`:

- `ExtensionIndexEntry`
- `ExtensionIndexDevice`
- index validation helpers

## User config

The existing `extensions.json` is currently a map of extension ID to
`ExtensionSettings`. Replace it with a versioned object and read the legacy map
for migration:

```json
{
  "version": 2,
  "indexes": [
    "https://lbirkert.github.io/notwig/extensions.json",
    "/Users/me/Dev/extensions.json"
  ],
  "settings": {
    "notwig.ext.devices": { "enabled": true, "trust": "Trusted" }
  }
}
```

- Index sources may be `http(s)://`, `file://`, or plain filesystem paths.
- Missing/corrupt config falls back to defaults and is reported in settings.
- Persist after index add/remove, install, uninstall, enable/disable, and trust
  changes.

## Install pipeline

All network and ZIP work happens off the UI thread:

1. Resolve the source:
   - index entry: resolve `url` against the index source, then download/read;
   - local import: read the selected ZIP.
2. Enforce download limits: max ZIP size (planned 64 MiB).
3. Open the ZIP in memory and locate `extension.json`:
   - root, or exactly one top-level directory containing `extension.json`;
   - reject ambiguous layouts.
4. Validate every archive path:
   - no absolute paths, `..`, drive prefixes, or symlinks;
   - cap entry count and total uncompressed size.
5. Parse and validate `ExtensionManifest`.
6. Extract into a staging directory under the extensions root.
7. Replace `<extensions-root>/<id>` with the staged directory.
8. Reload `ExtensionHost`, rebuild the engine, refresh the UI.
9. Report success/failure through notifications and the settings page.

Installing an already-installed ID replaces it. Default settings for a newly
installed extension are `enabled: true`, `trust: Trusted`.

Uninstall removes the extension directory and reloads the host. Whether the
per-extension settings row is also deleted is an open decision.

## Index refresh

- Refresh is explicit from Settings and also requested when the Extensions page
  is first shown with configured indexes.
- Fetch each index in the background; one failing source must not hide entries
  from the others.
- The UI shows per-source errors plus the merged entry list.
- Index fetch results are in memory; no disk cache in the first pass.

## Settings UI

Sections:

1. **Index sources**
   - list of URLs/paths with remove buttons;
   - text field + Add;
   - Refresh;
   - loading/error status.
2. **Available extensions**
   - name, vendor, version, capabilities, devices;
   - installed/version badge;
   - Install or Reinstall button.
3. **Installed extensions**
   - existing status, enable toggle, trust selector;
   - Uninstall;
   - error text.
4. **Import**
   - Import ZIP button;
   - drop hint;
   - dragging a `.zip` while the Extensions page is active installs it;
   - non-ZIP drops continue to the normal audio import path.

New `UiIntent`s:

- `RefreshExtensionIndex`
- `SetExtensionIndexes { indexes }`
- `InstallExtensionFromIndex { id }`
- `InstallExtensionZip { path }`
- `ImportExtensionZip` (opens the file picker)
- `UninstallExtension { id }`

## CI and official index

`docs/extensions.txt` contains one extension ZIP URL/path per line. Blank lines
and `#` comments are ignored.

`build-extensions.sh`:

- reads `docs/extensions.txt`;
- downloads/reads each ZIP;
- extracts `extension.json` from the ZIP root;
- adds the source `url`;
- writes a JSON array to `docs/extensions.json`;
- fails on malformed manifests or missing root metadata.

CI:

- installs the docs builder and runs `build-extensions.sh`;
- builds Markdown docs to HTML (planned `mdbook`);
- copies `extensions.json` into the output;
- publishes the result to GitHub Pages.

## Testing

- Index entry parsing, validation, relative URL resolution, and dedup.
- Config migration from the legacy map and round-trip of index sources.
- ZIP install: root layout, single-directory layout, path traversal rejection,
  size limits, ID mismatch, replacement, uninstall.
- Settings UI headless render with index entries, installed entries, errors, and
  drag/drop ZIP handling.
- Build script produces the expected array from a local fixture ZIP.

## Resolved decisions

1. **Multiple index sources**, with the default
   `https://lbirkert.com/notwig/extensions.json` when no config exists.
2. **CI packages `notwig-ext-devices`** into
   `docs/extensions/notwig.ext.devices.zip`, cached by a hash of the extension
   sources and `Cargo.lock`. CI appends the local ZIP path to a temporary copy
   of `docs/extensions.txt`; the build script converts it to the published
   relative URL.
3. **Reinstalling asks for confirmation** in the Settings UI. Local ZIP imports
   are an explicit user action and replace directly.
4. **Uninstall deletes the extension directory and its enabled/trust settings.**
5. **`mdbook`** builds the Markdown docs for GitHub Pages.
