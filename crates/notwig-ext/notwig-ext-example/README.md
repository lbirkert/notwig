# notwig-ext-example

A minimal extension package that demonstrates the Notwig command API. It
contributes `notwig.ext.example.humanize`, which nudges the timing and velocity
of selected MIDI notes.

The crate is intentionally small: it uses `notwig-ext-api` only, registers one
`CommandExtension`, and declares the command in `extension.json` with manifest
version 2. It doubles as the reference for command context, host edit families,
palette metadata, and default keybindings.

```sh
cargo build -p notwig-ext-example
cargo build -p notwig-ext-example --target wasm32-unknown-unknown
cargo test -p notwig-ext-example
```

See [`docs/02_extensions/06_command_api.md`](../../../docs/02_extensions/06_command_api.md).
