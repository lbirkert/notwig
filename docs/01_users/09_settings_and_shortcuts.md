# Settings and Shortcuts

Open **Settings** with **Cmd+,** or **FILE > Settings…**. The window has
three pages.

## Audio

The Audio page selects the output system:

- **Driver**: the available audio backends.
- **Device**: the output device for the selected driver, annotated with its
  input and output channel counts (for example `16-in, 16-out`). **System Out**
  follows the operating system's default output device, so it keeps working when
  the default changes. When that output device also exposes inputs, System Out
  uses it for recording as well so input and output stay on one clock domain.
  On first launch, the default driver and **System Out** are selected
  automatically.
- **Sample rate**: automatic or a rate reported by the device.
- **Render the project in a higher block size than monitoring**: enables the
  separate low-latency monitor path, the render-ahead worker, and the small
  device callback buffer.
- **Device Block Size**: the output device callback block. With higher-block
  project rendering enabled it can range from 16 samples up to the High latency
  buffer size. With it disabled, it is the single project/device block and is
  capped at 2048 samples.
- **High latency buffer**: the large project render block, always 16 to 8192
  samples in steps of 16. This control is disabled when higher-block project
  rendering is off.
- **Apply** opens the selected device and rebuilds the engine.
- **Revert** restores the last applied values.

Even when higher-block project rendering is off, the Low Latency monitoring
mode remains selectable; it behaves like **Enabled**.

The page shows both buffer sizes in samples and their resulting latency in
milliseconds. Driver, device, both buffer sizes, named channels, and the master
output persist per user in `audio.json`. The sample rate is project-scoped; if the
selected device does not support the project rate, Notwig uses the device's
default rate. If the saved driver or device is unavailable, Notwig falls back
to an available option and reports the problem on the page.

On the browser build the driver list includes **Web Audio**. It plays through
the browser's default output. Enabling audio also asks for microphone
permission so the input picker can show the browser's available input devices.

All audio settings are applied through the apply action. Changing the device
while a project is open can interrupt playback.

### Inputs and outputs

The **Inputs & Outputs** section names the interface channels used by tracks:

- **Add mono input**, **Add stereo input**, **Add mono output**, and
  **Add stereo output** create a named channel.
- Each row names the channel. Mono channels select one physical channel; stereo
  channels select independent left and right channels.
- Named channels always use the single device selected in **Audio System**;
  there is no per-channel device selector.
- **Remove** deletes a named channel. Tracks keep their physical channel
  selection, so remove a channel only when its tracks have been reassigned.
- **Master output** chooses which named output carries the stereo mix. The
  default output uses the first two channels of the selected device.

The audio track input picker lists only the named inputs configured here. A
stereo input records its left and right channels; a mono input records one
channel into both sides of the stereo asset.

## Shortcuts

The Shortcuts page lists every command grouped by category.

- Use the search field to filter by command name, ID, or category.
- Click **+** beside a command and press a key combination to assign it.
- A command can have several bindings.
- Click the **×** on a binding to remove it.
- **Reset Defaults** restores the built-in bindings.

Bindings persist per user in `shortcuts.json`. Arrow keys inside the MIDI
editor, Escape to close editors, and mouse gestures are separate from the
command catalog and are not rebindable.

### Default bindings

`Cmd` is the Command key on macOS.

| Category | Command | Default |
| --- | --- | --- |
| Application | Quit Notwig | Cmd+Q |
| Application | New Project | Cmd+N |
| Application | Open Project | Cmd+O |
| Application | Save | Cmd+S |
| Application | Save As | Cmd+Shift+S |
| Transport | Play / stop | Space |
| Transport | Stop | Shift+Space |
| Transport | Loop time or clip selection / toggle loop | Cmd+L |
| Edit | Undo | Cmd+Z |
| Edit | Redo | Cmd+Shift+Z |
| Edit | Select All | Cmd+A |
| Edit | Copy | Cmd+C |
| Edit | Cut | Cmd+X |
| Edit | Paste | Cmd+V |
| Edit | Duplicate | Cmd+D |
| Edit | Delete | Delete or Backspace |
| Edit | Join Clips | Cmd+J |
| Arrangement | Add Instrument Track | Cmd+T |
| Arrangement | Add Audio Track | Cmd+Shift+T |
| Arrangement | Group Tracks | Cmd+G |
| Arrangement | Disable Track | Alt+A |
| Arrangement | Fit Arrangement | Cmd+0 |
| View | Open Settings | Cmd+, |
| View | Cancel / close editor | Esc |

Commands without a default binding can be assigned in Settings: Import Audio,
Audio Settings, Render Mixdown, Toggle Loop, Toggle Info Panel, Toggle Device
Editor, and Toggle Clip Editor.

## Extensions

The Extensions page installs and manages WASM and native extensions.

- **All**, **Available**, and **Installed** filter the list.
- **Add .ZIP** installs a packaged extension from disk.
- Dropping a `.zip` file onto the Extensions page also installs it.
- **Manage Indices** adds or removes JSON index sources used for **Available**
  extensions.
- Platform tags show which targets an extension supports: WASM, macOS,
  Windows, and Linux.
- Extensions without a build for the current platform are dimmed and cannot be
  installed.
- An available extension has **Install**.
- An installed extension can be **Disabled**, **Enabled**, **Reinstalled**, or
  **Uninstalled**. If its installed archive hash differs from the current index
  hash, an **Update** button appears below **Uninstall**.

Removing an extension removes its files and settings. Extension devices already
placed in a project are unavailable until the extension is installed again.

## DSP analysis

Click the CPU load readout in the top bar to open the DSP window. It shows:

- callback history;
- average and maximum load;
- the deadline;
- jitter;
- output latency.

Use it to check whether the audio callback has headroom before adding more
devices.

## Notifications and errors

Notifications appear at the top right. Error notifications are red, warnings
use the accent color, and info messages are muted. A notification can be
dismissed with its close button. Long operations such as extension installs
can show progress in the same overlay.

The status bar and info panel show the result of the latest operation. A
failed command leaves the document unchanged and reports the reason.
