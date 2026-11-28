# Command API

Status: implemented. Extensions can contribute
commands with runtime metadata, a bounded context, host-owned edit families,
command palette integration, and default keybindings. The reference package is
`crates/notwig-ext/notwig-ext-example`.

Commands supplement devices and audio sub-editors; they do not give an
extension access to `ProjectDoc`, the audio callback, or host UI. The host validates the result,
applies it through `notwig_core::apply`, and records one undo entry.

## Runtime metadata

Commands are declared by the guest at load time, not by the package manifest.
The manifest only needs `manifest_version: 2` and the `command` capability:

```json
{
  "manifest_version": 2,
  "id": "com.example.midi-tools",
  "name": "MIDI Tools",
  "vendor": "Example",
  "version": "0.1.0",
  "api_version": 9,
  "capabilities": ["command"]
}
```

The host reads the command descriptor from `notwig_ext_registrations` after the
load phase. The command ID must be stable and namespaced by the extension. The
host lists it in the command palette and shortcut editor. Default bindings are
only suggestions; the host never lets an extension default steal a binding
already used by a built-in command.

The `when` expression is declarative, not executable code. Supported fields are:

- `focus` compared with `timeline`, `audio_editor`, `midi_editor`,
  `device_editor`, or `panel`;
- `selection.notes`, `selection.clips`, `selection.devices`, and
  `selection.slices` as counts;
- `selection.time` and `snap` as booleans;
- `revision` as a number.

Use `&&`, `||`, `!`, parentheses, and `==`, `!=`, `<`, `<=`, `>`, `>=`.

## Handler

Register a command from the same macro entrypoint used by devices:

```rust
use notwig_ext_api::{
    EP_COMMANDS, CommandContext, CommandDescriptor, CommandEdit, CommandEffect,
    CommandExtension, ExtensionBuilder, notwig_ext,
};

pub const HUMANIZE_COMMAND_ID: &str = "com.example.midi-tools.humanize";

pub struct HumanizeCommand;

impl CommandExtension for HumanizeCommand {
    fn new() -> Self {
        Self
    }

    fn descriptor() -> CommandDescriptor {
        CommandDescriptor {
            id: HUMANIZE_COMMAND_ID.into(),
            title: "Humanize Notes".into(),
            category: "MIDI".into(),
            description: "Nudge the timing and velocity of selected notes.".into(),
            when: Some("focus == midi_editor && selection.notes > 0".into()),
            default_bindings: vec!["Ctrl+Alt+H".into()],
            extension_id: None,
        }
    }

    fn execute(&mut self, context: &CommandContext) -> CommandEffect {
        let Some(clip_id) = context.active_clip else {
            return CommandEffect::Error {
                message: "Open a MIDI clip first.".into(),
            };
        };
        let notes = context
            .selection
            .notes
            .iter()
            .cloned()
            .map(|mut note| {
                // Modify existing notes; the host validates and applies them.
                note.velocity = notwig_core::Norm01(0.5);
                note
            })
            .collect();
        CommandEffect::Apply {
            edits: vec![CommandEdit::MidiNotes {
                clip_id,
                notes,
                removed: Vec::new(),
            }],
            ui: Vec::new(),
        }
    }
}

#[notwig_ext(load)]
pub fn load(extension: &mut ExtensionBuilder) {
    extension.register_extension_factory::<HumanizeCommand>(EP_COMMANDS);
}
```

The handler runs on the UI/app thread. It must return promptly, must not perform
I/O, and must not block. Use a host job/analysis service later when a command
needs long-running work.

## Context

`CommandContext` is an immutable, bounded snapshot:

- document revision;
- focused editor;
- selected tracks, clips, devices, time range, notes, and audio slices;
- active track, clip, and device;
- playhead, tempo, sample rate, and snap state.

Selected notes and slices are capped. If a selection is too large, the command
is disabled rather than receiving a truncated view. The host checks the revision
before applying an effect; a stale context fails without changing the document.

The extension never receives the full `ProjectDoc`.

## Effects

`CommandEffect` is one of:

- `None`;
- `Apply { edits, ui }`;
- `Error { message }`.

Supported edit families are:

| Family | Result |
| --- | --- |
| `MidiNotes` | Upsert/remove notes in a MIDI clip. |
| `AudioSlices` | Replace an audio clip's slices. |
| `AudioCurves` | Change or clear an audio clip's canonical gain or pan curve. |
| `ClipFields` | Change clip range, name, mute, or color. |
| `TrackMixer` | Change volume, pan, mute, or solo. |
| `DeviceParam` | Set one device parameter. |

UI requests can show a notification or focus an editor. The host applies the
whole effect as one undo group. An extension cannot submit an arbitrary
`CommandPayload`, allocate durable IDs, or mutate the document directly.

## Palette and shortcuts

Press `F1` or `Cmd/Ctrl+Shift+P` to open the command palette. The palette lists
built-in and extension commands that match the query by title, category, ID, or
description, and shows the effective keybinding. Commands whose `when`
condition is currently false stay visible but muted and cannot run, so
extension commands remain discoverable outside their active editor context.
Selecting an available extension command emits a command intent with the
current context.

Extension default bindings are added only when the command has no configured
entry and the chord is free. Settings -> Shortcuts lists the same commands and
persists bindings by string command ID, so bindings survive extension reloads.

## Testing

- Test `execute` directly with a constructed `CommandContext`; no host is
  required.
- Test the manifest separately with `ExtensionManifest::validate_source`.
- Test the real host path with an inline WAT module or a built package; see
  `crates/notwig-ext/notwig-ext/tests/commands.rs`.
- Add a headless UI test when a command changes palette or shortcut behavior.

`notwig-ext-example` contains the humanize command and runs the same tests as a
normal extension crate.
