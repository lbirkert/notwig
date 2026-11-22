# notwig-ext-cli

Build and scaffold Notwig extension packages.

## Commands

```sh
notwig-ext-cli new <path> [--name <package>] [--id <extension-id>]
notwig-ext-cli init [path] [--name <package>] [--id <extension-id>]
notwig-ext-cli build <target> [--path <dir>] [--out-dir <dir>] [--name <zip-name>]
notwig-ext-cli targets
```

`new` creates a new extension directory. `init` adds the extension files to an
existing directory. Both generate:

- `Cargo.toml` with workspace-free `cdylib`/`rlib` settings and path deps on
  `notwig-core`, `notwig-ui`, and `notwig-ext-api`;
- `src/lib.rs` with a working `#[notwig_extension]` gain device;
- `extension.json` without `entrypoints` or `includes`;
- a default `icon.png`.

`build` searches for `extension.json` from `--path` (or the current directory)
upward, runs `cargo build --release --target <rust-triple>`, transforms the
manifest, and writes a ZIP. If `--name` is omitted, the ZIP is named
`<extension-id>-<target>.zip`.

See the [parent README](../README.md) for the manifest format, supported
targets, and include gating rules.
