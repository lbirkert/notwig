# notwig-ext-example

A minimal extension package that demonstrates the Notwig command and audio
sub-editor APIs. It registers `notwig.ext.example.humanize`, which nudges the
timing and velocity of selected MIDI notes, and `notwig.ext.example.pan`, a
Clip Pan sub-editor for audio clips.

The crate registers one `CommandExtension` and one `AudioSubEditorExtension`
through `EP_COMMANDS` and `EP_NOTWIG_AUDIO_EDITOR_SUB_EDITOR`. Both descriptors
come from the implementation traits. The pan editor draws the shared curve
editor over a waveform underlay and commits a host-owned
`CommandEdit::AudioCurves` pan change. The host owns the context snapshot,
validation, revision check, and undo boundary. It doubles as the reference for
command context, host edit families, palette metadata, default keybindings, and
extension-rendered audio-editor contributions.

```sh
cargo build -p notwig-ext-example
cargo build -p notwig-ext-example --target wasm32-unknown-unknown
cargo test -p notwig-ext-example
```

The crate root follows the extension file convention: `lib.rs` contains only
wiring and lifecycle entrypoints, in that order, and the lifecycle functions are
the final items. Implementations live in `humanize.rs`, `pan.rs`, and `point.rs`;
integration tests live under `tests/`.

See [`docs/02_extensions/06_command_api.md`](../../../docs/02_extensions/06_command_api.md)
and [`docs/02_extensions/07_audio_sub_editor_api.md`](../../../docs/02_extensions/07_audio_sub_editor_api.md).
