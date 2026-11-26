# notwig-ext-example

A minimal extension package that demonstrates the Notwig command and audio
sub-editor APIs. It contributes `notwig.ext.example.humanize`, which nudges the
timing and velocity of selected MIDI notes, and `notwig.ext.example.pan`, a
Clip Pan sub-editor for audio clips.

The crate is intentionally small: it uses `notwig-ext-api` only, registers one
`CommandExtension`, and declares both contributions in `extension.json` with
manifest version 2. The pan editor is a shared-curve contribution: the host owns
the global curve editor, the canonical `pan_curve`, preview, commit, and undo.
The extension supplies only the selector metadata and value mapping. It doubles
as the reference for command context, host edit families, palette metadata,
default keybindings, and shared audio-editor curve contributions.

```sh
cargo build -p notwig-ext-example
cargo build -p notwig-ext-example --target wasm32-unknown-unknown
cargo test -p notwig-ext-example
```

See [`docs/02_extensions/06_command_api.md`](../../../docs/02_extensions/06_command_api.md)
and [`docs/02_extensions/07_audio_sub_editor_api.md`](../../../docs/02_extensions/07_audio_sub_editor_api.md).
