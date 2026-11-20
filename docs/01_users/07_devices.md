# Devices

Devices process a track in a chain. Notwig ships three stock devices and can
host VST3, VST2, AU, and WASM extension devices.

## The device editor

Open the device editor in any of these ways:

- double-click a track header;
- click the sliders icon or the **Show Device Editor** button in the status bar;
- use the device editor shortcut if you assign one.

The editor shows the selected track's main device chain. Each card represents
one device, left to right in processing order.

## Adding and removing devices

- Click a **+** insertion point between device cards to open the Add Device
  browser.
- Search by name, vendor, or type ID.
- Use the arrow keys to move through the list and **Enter** to insert.
- Stock devices, extension devices, and scanned plugins are grouped separately.
- Click a device card to select it.
- Double-click a card to collapse or expand it.
- Right-click a card and choose **Remove device** to remove it.
- Click the power button on the card handle to bypass or enable the device.
- Bypassed devices keep their state and are skipped by the engine.

Device drag reordering is not implemented. To change order, remove and reinsert
the device at the desired insertion point.

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

Notwig scans VST3, VST2, and AU plugins in a separate process and caches the
result. Scanned plugins appear under **PLUGINS** in the Add Device browser, with
a **VST3**, **VST**, or **AU** tag so you can tell formats apart.

- Select a plugin to insert it into the current chain.
- A plugin card with a window button can open its native editor on macOS,
  including VST2 editors.
- Plugin state is saved with the project and restored when instances are
  rebuilt.
- Device latency reported by the plugin is compensated across the project.
- The generic parameter view lists plugin parameters. A plugin's own editor is
  the better surface for detailed editing.

Plugin scanning, blacklisting, and editor behavior are macOS-first. Real plugin
load and process are not covered by the automated test suite yet.

## Extension devices

WASM extension devices are installed from **Settings > Extensions**. Once
installed and loaded, they appear under **EXTENSIONS** in the Add Device browser.

- Extension devices provide their own DSP and can provide a custom panel.
- If an extension cannot draw a custom panel, Notwig uses the generic
  parameter view as a fallback.
- In the browser build, only `Trusted` extensions are supported. The native
  build additionally supports `Limited` and `DrawOnly` trust levels.
- Extension trust level controls whether the device may process audio and how
  much memory and execution time it gets.

See [Extension Overview](../02_extensions/00_overview.md) for installation and
[Device API](../02_extensions/03_device_api.md) for the authoring contract.

## Latency and processing

The engine compensates device latency across tracks, group chains, sends, and
bypassed devices. Inactive tracks are excluded from processing, and a track
that is re-enabled rejoins the compensated mix.

Next: [Transport, Loop, and Render](08_transport_and_render.md).
