# Getting Started

## Before you start

Notwig is under active development. It can arrange, edit, mix, record, and
render audio and MIDI.

There are two ways to run it:

- **Native macOS build.** Install a recent Rust toolchain and the platform
  audio and plugin SDKs used by CPAL and the VST3/VST2/CLAP/LV2/AU host. LV2
  also needs the system `lilv` library (`brew install lilv`). From the
  repository root, run `cargo run -p notwig-app`.
- **Browser build.** Build `crates/notwig-web` with `wasm-pack` and serve the
  generated static files. Native plugins and native extension libraries are
  unavailable; only `wasm` extension targets run in the browser.

The repository `README.md` contains the exact build commands. The rest of this
page assumes the application is already running.

## Create or open a project

A Notwig project is a directory bundle ending in `.notwig`. Use the **FILE**
menu in the top-left corner:

- **Recent Projects** opens a submenu of recently used bundles.
- **New Project** creates an empty project with a master track.
- **Open Project** opens a bundle.
- **Save Project** and **Save As** write the bundle.
- **Download Project** exports the browser project as a `.notwig.zip` (browser
  build only).
- **Import Audio…** adds an audio file to the project.

On first launch, use **New Project**. Notwig does not prompt for a save
location until you save, so choose a name and folder when the save dialog
appears.

## Build a first arrangement

1. Add a track from **+ Add track** below the track list. Choose an audio or
   instrument track, or right-click the add row for more track kinds.
2. Create a clip in one of two ways:
   - double-click an empty lane to create a four-beat clip;
   - drag an audio file from Finder onto an audio track.
3. Double-click a clip to open its editor. MIDI clips open the MIDI editor and
   audio clips open the audio editor.
4. Press **Space** to play and stop.
5. Use **Cmd+S** to save the project.

The vertical play-start marker in the arranger is where playback begins. Click
the ruler to move it. The playhead moves during playback and returns to the play
start when you stop.

## A quick tour

- The **app bar** shows the project name card and the centered Notwig title.
- The **top bar** below it contains the file menu, transport controls, tempo and
  time signature, playhead position, CPU load, and master meter.
- The **arranger** is the main timeline. Track headers are on the left, clip
  lanes and the timeline are on the right.
- The **info panel** on the left shows the current selection: a track, clip,
  device, time range, or file.
- The **device editor** opens from the status bar or by double-clicking a track
  header. It shows the selected track's device chain.
- The **clip editor** opens at the bottom for the selected audio or MIDI clip.
- The **status bar** toggles the info panel and editors.
- **Settings** contains audio, shortcuts, and extensions.

## If something goes wrong

Notwig writes an autosave while a project is dirty and keeps a session lock
while the bundle is open. If a project was not closed cleanly, the next open
offers the latest autosave. See [Projects and Recovery](02_projects_and_recovery.md)
for the full behavior.

If the audio engine stops unexpectedly, an overlay appears above the arranger.
Use **Restart engine** to rebuild it. If no audio device is available, open
**FILE > Settings…** and select a working driver and device.

Next: [Workspace Overview](01_workspace_overview.md).
