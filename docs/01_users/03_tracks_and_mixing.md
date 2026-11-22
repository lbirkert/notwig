# Tracks and Mixing

## Track kinds

Notwig has five track kinds:

| Kind | Purpose | Lane |
| --- | --- | --- |
| Audio | Hosts audio clips and audio devices. | Yes |
| Instrument | Hosts MIDI clips and instruments or effects. | Yes |
| Group | Sums child tracks and applies its own device chain. | No |
| Bus FX | A bus-style track kind for device processing. | No |
| Master | The final output. Created with every project and cannot be deleted. | No |

Only audio and instrument tracks draw clip lanes. Group and bus tracks can hold
devices and can be nested.

## Creating, selecting, and moving tracks

- Click **+ Add track** below the track list to add an audio track.
- Right-click **+ Add track** to choose audio, instrument, or group.
- Right-click a track header for the same choices plus grouping and deletion.
- Click a track header to select it. Cmd-click adds a track. Shift-click selects
  a range in the visible order.
- Drag a track name to reorder it. Drop it above or below another track, or into
  a group. The white drop line shows the destination and nesting depth.
- Drag a group's triangle to collapse or expand its children.
- Press **Cmd+G** to group the selected tracks under a new group track.

The master track is pinned at the bottom of the arranger.

## Track controls

Each track header contains:

- the track name, type icon, and color marker;
- a volume fader;
- **R**, **M**, and **S** buttons;
- a vertical peak meter;
- a menu button for the track context actions.

**R** arms an audio track for recording. An armed track monitors its selected
input through its device chain and mixer. **M** mutes the track. **S** solos it.
Solo respects group ancestry and descendants, so soloing a group keeps its
children audible.

The info panel exposes the same controls and adds:

- an editable name;
- a color picker;
- a comment field;
- an input picker for audio tracks, including named mono and stereo inputs from
  the audio settings;
- the active-state power button;
- pan and volume with a peak hold readout;
- the track's device list.

Audio tracks record one selected mono or stereo input. Per-track input selection
uses a driver, device, and channel (or channel pair), so an interface's inputs
can be assigned to different tracks. Named inputs configured in
**Settings > Audio** are listed before the raw hardware channels.

Click a peak readout to reset its held value.

While a track still has its automatic name, inserting an instrument renames an
instrument track to that instrument's display name. Renaming the track yourself
stops this automatic naming, and other track kinds keep their names when you add
devices.

## Active state

The power button in the info panel toggles a track's active state. Inactive
tracks are dimmed and their devices are skipped. Use this to compare a processed
track against the rest of the mix without removing its devices.

## Grouping and nesting

Group tracks sum their children. A child can be an audio, instrument, or another
group track. Move a track into a group by dragging its header onto the group's
row or between its children.

Group ancestry affects mute and solo. It also affects latency compensation:
device latency inside a group and on the group itself is compensated before the
final mix.

## Mixing limits

Notwig mixes through track headers and the info panel. There is no dedicated
mixer view.

The engine supports internal track-to-track routing, but routing controls are
not exposed in the UI. Sends, return tracks, and sidechains are not available.
Mix with volume, pan, mute, solo, and the device chain.

## Audio format

The engine and asset pipeline are stereo. Mono files and mono recordings are
converted into the stereo path, while stereo inputs are recorded as stereo. More
than two channels is not a supported track format yet.

Next: [Arranger](04_arranger.md).
