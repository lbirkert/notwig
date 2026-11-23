# Devices

Devices process a track in a chain. Notwig ships three stock devices and can
host VST3, VST2, CLAP, LV2, AU (v2/v3), and WASM or native extension devices.

## The device editor

Open the device editor in any of these ways:

- double-click a track header;
- click the sliders icon or the **Show Device Editor** button in the status bar;
- use the device editor shortcut if you assign one.

The editor shows the selected track's main device chain. Each card represents
one device, left to right in processing order.

## Adding, moving, and removing devices

- Click a **+** insertion point between device cards to open the Add Device
  browser.
- Search by name, vendor, or type ID.
- Use the arrow keys to move through the list and **Enter** to insert.
- Stock devices, extension devices, and scanned plugins are grouped separately.
- Click a device card to select it.
- Double-click a card to collapse or expand it.
- Drag a card by its handle and drop it on an insertion point to reorder it,
  including between nested Chain children.
- Drag a card onto an arranger lane to append it to that track's main chain.
  MIDI-only devices are rejected by incompatible lanes.
- Right-click a card to **Copy device**, **Cut device**, **Paste device**,
  **Deactivate device**, or **Remove device**. Cmd+C, Cmd+X, and Cmd+V operate
  on the selected devices when a device selection is active.
- Click the power button on the card handle to enable or disable (bypass) the
  device. Disabled devices keep their state and stay loaded, but their audio
  processing is skipped.
- Press **Alt+A** to activate or deactivate the selected devices, or the
  selected tracks when no device is selected. Deactivation unloads the device
  (including its plugin) to release CPU and memory. Deactivated cards are dimmed
  and keep their document state. Shortcuts do nothing while a clip or MIDI
  editor has focus.

## Device activation and bypass

Notwig has two independent device switches:

| Switch | Shortcut / control | Plugin lifecycle | Audio |
| --- | --- | --- | --- |
| Enable / disable (bypass) | Power button | Stays loaded | Dry signal, no DSP |
| Activate / deactivate | **Alt+A** | Deactivated plugins unload | Deactivated devices are absent from the chain |

Bypassed devices keep their planned latency so enabling and disabling does not
shift latency compensation. Copied and pasted devices receive new IDs, including
new IDs for the child chains of a Chain container.

## Stock devices

### Tool

A utility processor for gain and stereo placement:

- input gain;
- output gain;
- pan;
- stereo width;
- polarity;
- dry/wet.

Use it to trim a track, check polarity, or narrow a stereo source without
inserting a full channel strip.

### EQ

A four-band equalizer. Each band can be a bell, low shelf, high shelf, high pass,
or low pass. The device also provides a global shift and output gain.

Drag a band number to adjust gain and frequency. Drag the band handle to adjust
Q and frequency. Bands can be enabled individually. The EQ has a dedicated
display in the device editor and a dedicated band list in the info panel.

### Chain

A nested container with its own device chain, input gain, output gain, and
dry/wet. Inserting a Chain inside another chain creates a nested group. Use it
to process a group of devices as one unit or to blend parallel processing with
the dry/wet control.

## Plugins

Notwig scans VST3, VST2, CLAP, LV2, AU v2, and AU v3 plugins in a separate
process and caches the result. Scanned plugins appear under **PLUGINS** in the
Add Device browser, with a **VST3**, **VST**, **CLAP**, **LV2**, **AU**, or
**AU3** tag so you can tell formats apart.

- Select a plugin to insert it into the current chain.
- A plugin card with a window button can open its native editor on macOS,
  including VST2 and LV2 editors.
- Plugin state is saved with the project and restored when instances are
  rebuilt.
- Device latency reported by the plugin is compensated across the project.
- The generic parameter view lists plugin parameters. A plugin's own editor is
  the better surface for detailed editing.

Plugin scanning, blacklisting, and editor behavior are macOS-first. LV2
discovery requires the system `lilv` library (for example `brew install lilv`).
On desktop, the audio engine, plugin DSP, and plugin editor windows run in a
separate engine process. If a plugin crashes, the engine process exits and the
app shows a restart notice with a **Restart engine** button; the project and UI
stay alive. Plugin metadata is also probed in a child process before loading.
Real plugin load and process are not covered by the automated test suite yet.

## Extension devices

Extension devices are installed from **Settings > Extensions**. Once installed
and loaded, they appear under **EXTENSIONS** in the Add Device browser.

- Extension devices provide their own DSP and can provide a custom panel.
- The bundled `notwig.ext.devices` pack adds Limiter, Gate, Reverb, Synth, and
  Gain devices.
- Reverb is a container with a wet FX chain: expand the chain tab on its card
  and insert devices to process the reverb's wet signal. The device's Mix knob
  blends the processed wet signal with the dry input.
- Extension containers use the same chain tab and nested device editing as the
  stock Chain device. Nested parameters, latency, and bypass work the same way.
- If an extension cannot draw a custom panel, Notwig uses the generic
  parameter view as a fallback.
- On desktop, the app uses the first package target it can run, preferring a
  native shared library over WASM when the manifest lists one first.
- The browser build loads `wasm` entrypoints only.
- WASM devices run with a memory limit and per-call execution limits. Native
  shared libraries are not sandboxed and run with the app's privileges, so only
  install them from sources you trust.

### Browser extensions

The browser build uses the same extension index as the native app. The default
index is `https://notwig.lbirkert.com/extensions.json`; change or add sources
under **Settings > Extensions > Manage Indices**. The list is stored in browser
local storage.

- Installing from the index or importing a `.ZIP` extracts the package into a
  `notwig-extensions/<id>/` folder in the Origin Private File System (OPFS).
- Extensions installed in OPFS are discovered and loaded when the app starts.
- Uninstalling removes the extension folder. Packages installed as ZIP files by
  older browser builds are migrated into extension folders on the next start.

See [Extension Overview](../02_extensions/00_overview.md) for installation and
[Device API](../02_extensions/03_device_api.md) for the authoring contract.

## Latency and processing

The engine compensates device latency across tracks, group chains, sends, and
bypassed devices. Bypassed devices keep their planned latency so toggling them
does not shift the mix. Deactivated devices are not instantiated and contribute
no latency until they are activated again. Inactive tracks are excluded from
processing, and a track that is re-enabled rejoins the compensated mix. See
[Routing and Delay Compensation](../03_development/21_routing_and_pdc.md) for
the latency labels and the known limits.

Next: [Transport, Loop, and Render](08_transport_and_render.md).
