# notwig-ext-example

A minimal extension package that demonstrates the Notwig command and audio
sub-editor APIs. It contributes `notwig.ext.example.humanize`, which nudges the
timing and velocity of selected MIDI notes, and `notwig.ext.example.pan`, a
custom Clip Pan sub-editor for audio clips.

The crate is intentionally small: it uses `notwig-ext-api` only, registers one
`CommandExtension` and one `AudioSubEditorExtension`, and declares both
contributions in `extension.json` with manifest version 2. The pan editor draws
its own panel inside the host audio editor and commits a host-owned
`CommandEdit::AudioCurves` pan change. The host owns the context snapshot,
validation, revision check, and undo boundary. It doubles as the reference for
command context, host edit families, palette metadata, default keybindings, and
extension-drawn audio-editor contributions.

```sh
cargo build -p notwig-ext-example
cargo build -p notwig-ext-example --target wasm32-unknown-unknown
cargo test -p notwig-ext-example
```

See [`docs/02_extensions/06_command_api.md`](../../../docs/02_extensions/06_command_api.md)
and [`docs/02_extensions/07_audio_sub_editor_api.md`](../../../docs/02_extensions/07_audio_sub_editor_api.md).
