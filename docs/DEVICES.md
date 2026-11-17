# Device UI Contracts

Current implementation: `crates/notwig-ui/src/device/{panel,browser,params,eq}.rs` and the device section of `info.rs`.

Related: [`FRONTEND.md`](./FRONTEND.md) for panel placement and selection routing.

## 1. Placement and scope

The device panel is the default bottom editor. It shows the selected track's main device chain.

- Height: 260 px, non-resizable, collapsible by dragging its top handle down.
- Horizontally scrollable.
- If no track is selected, it shows `Select a track to inspect its device chain.`
- It is hidden while the audio clip editor is open.

Device ordering is insertion-only in the current UI. Devices are inserted at a chosen index with the `+` insertion points. There is currently no drag-to-reorder device gesture.

## 2. Device cards

Each device is drawn as a card with a 32 px handle and a body.

Card widths:

- EQ: 560 px.
- Chain: 150 px.
- All other devices: 300 px.
- Collapsed device: 32 px (handle only).

Card height is 220 px.

### Handle

The handle contains, top to bottom:

- Power/bypass button.
- Vertical device name (up to 20 characters).
- Plugin-window button for VST3/AU devices.

Interactions:

- Click the handle: select device.
- `Cmd`/`Shift` click: additive/toggle device selection.
- Double-click the handle: collapse/expand the card.
- Right-click: `Remove device`.
- Power click: toggle bypass.
- Plugin-window click: emit `OpenPluginWindow`.

Selection and collapse are independent. Collapsing a card does not change selection.

## 3. Insertion and add-device browser

A `+` insertion point appears before, between, and after devices in every chain. Clicking it opens the floating `Add Device` window for that track, chain, and index.

The browser:

- Opens with search focused.
- Lists stock devices first: Tool, EQ, Chain.
- Then lists available plugins from the runtime plugin catalog.
- Filters by name, vendor, or type ID, case-insensitively.
- Supports `ArrowUp`/`ArrowDown` navigation and `Enter` to insert.
- Clicking a row inserts it and closes the browser.
- Stock entries emit `InsertStockDevice`; plugin entries emit `InsertPlugin`.
- Plugin insertion also attempts to open the native plugin window.

There is no drag-and-drop from the browser in the current UI.

## 4. Chain nesting

A container device (`Chain`) owns a child chain. Its card body shows a `CHAIN` tab.

- The tab toggles expansion of the first child chain.
- Expanded child devices render inline to the right of the container, recursively.
- Nested device groups get a bracket above them.
- Bracket colours cycle by depth (blue, green, amber, purple, teal).
- Collapsing the container card hides its body and stops rendering its children.
- The chain tab remains the only expansion control.

## 5. Device bodies

### EQ (`notwig.eq`)

The EQ body is a graph-first editor:

- Four bands, each with enable, type, frequency, gain, and Q.
- Frequency axis: 20 Hz to 20 kHz, logarithmic.
- Gain axis: symmetric ±30, ±20, or ±10 dB range selector.
- The response curve is computed from the DSP magnitude response, not a UI approximation.
- Each enabled band has a numbered point. The selected band is highlighted and has a vertical guide line.
- Dragging a point changes frequency (x) and gain (y).
- Dragging the `↔` handle changes Q (and frequency with x).
- Double-clicking an enabled point disables that band.
- Clicking a disabled band's slot enables it at the clicked frequency/gain.
- Band controls below the graph: band selector, enable, type, Q, gain, frequency.
- Header controls: shift (semitones, -12..12) and output gain (dB, -24..24).
- Analyzer options live in the inspector, not the card: pre/post toggles, resolution, speed, tilt, range.

### Tool (`notwig.tool`)

- Polarity invert L and R, plus `Swap L/R`.
- Knobs: Volume, Gain, Pan, Width.
- Uses the stock `ParamSpec` for ranges, defaults, normalization, and value formatting.

### Chain (`notwig.chain`)

- Wet Gain and Mix knobs.
- Plus the nested-chain UI described above.

### Other devices

Generic parameter editor, described next.

## 6. Generic plugin parameter editor

Used for any device that is not EQ, Tool, or Chain.

- A search bar filters visible parameters by name.
- Only parameters with `flags.visible` are listed.
- The last-touched parameter is pinned above the scrolling list and highlighted.
- Rows are virtualised by the scroll area.
- Each row has a normalized knob, name, and formatted value.
- Bipolar parameters (range crosses zero) draw their arc from the centre.
- Values are formatted by unit: dB, Hz/kHz, %, seconds, semitones, or plain.

Knob/value interaction (`widgets::value_drag`):

- Vertical drag changes the value; the drag is anchored at the press origin.
- `Shift` while dragging reduces speed to 10%.
- Double-click resets to the parameter default.
- `Cmd`-click opens a text entry field; `Enter`/focus loss commits a clamped value, `Escape` cancels.
- Continuous edits emit `SetParam` as the normalized value changes.

If metadata is missing, the card shows `No parameter metadata. Relink or install the plugin.`

## 7. Device inspector

Selecting a device drives the left inspector.

- Shows multi-device count/list and the primary device when several are selected.
- Shows power state, name, type ID, and active/bypassed status.
- EQ devices show a compact band list (enable, type, frequency, gain, Q) and analyzer settings.
- Other devices show the first 12 spec parameters with formatted values.
- The inspector device list on a track can be used to select/bypass devices without the bottom panel.

## 8. Plugin hosting UI

- Plugin descriptors come from the app's runtime plugin catalog.
- Device metadata is resolved lazily by loading the plugin and describing its parameters.
- VST3/AU devices expose a native-window button in the handle; the app opens an in-process OS window.
- Native-window parameter changes are polled by the app and applied as `SetParam`; the UI marks the parameter as last-touched.
- The generic parameter view remains available even when a native editor exists.
- There is no custom per-plugin UI other than the native editor.

## 9. Invariants

1. All parameter writes go through `SetParam`; the UI reads current values from the document.
2. Device identity is `DeviceId`; collapse, expansion, and search are UI-only state.
3. Chain expansion is UI-only and does not change device structure.
4. The power control is bypass, not removal.
5. Device removal is available from the handle context menu.
6. Stock devices have dedicated UI only for Tool, EQ, and Chain; all others use the generic parameter editor.
7. Insertion always targets an explicit chain and index.
