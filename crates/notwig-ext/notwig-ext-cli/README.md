# notwig-ext-cli

Build and scaffold Notwig extension packages.

## Commands

```sh
notwig-ext-cli new <path> [--name <package>] [--id <extension-id>]
notwig-ext-cli init [path] [--name <package>] [--id <extension-id>]
notwig-ext-cli build --entrypoint <target> [--path <dir>]
notwig-ext-cli build --package [target...] [--path <dir>] [--out-dir <dir>] [--name <zip-name>]
notwig-ext-cli targets
```

Run `notwig-ext-cli --help` or `notwig-ext-cli <command> --help` for full
option help. `new` creates a new extension directory. `init` adds the extension
files to an existing directory. Both generate:

- `Cargo.toml` with workspace-free `cdylib`/`rlib` settings and path deps on
  `notwig-core`, `notwig-ui`, and `notwig-ext-api`;
- `src/lib.rs` with a working `#[notwig_extension]` gain device;
- `extension.json` without `entrypoints` or `includes`;
- a default `icon.png`.

`build` searches for `extension.json` from `--path` (or the current directory)
upward. It has two modes:

- `--entrypoint <target>` builds only that target's entrypoint binary and leaves
  it in Cargo's standard `target/<rust-triple>/release/` directory.
- `--package [target...]` does not compile. It packages the entrypoint binaries
  already present in Cargo's target directory into one ZIP. With no target
  arguments it includes every available target; passing targets limits the
  package to those targets.

Build on each target platform or VM, then copy the produced
`target/<rust-triple>/release/` artifact into the same target directory on the
packaging machine:

```sh
# On the Windows/macOS/Linux/WASM build machine:
notwig-ext-cli build --entrypoint windows-x86_64
notwig-ext-cli build --entrypoint macos-aarch64
notwig-ext-cli build --entrypoint wasm

# On the packaging machine, after copying the target artifacts:
notwig-ext-cli build --package --out-dir dist
```

Without `--name`, the ZIP is named `<extension-id>.zip`, or
`<extension-id>-<target>.zip` when only one entrypoint is available. Missing
Rust targets are installed through `rustup target add` for `--entrypoint`
builds by default (disable with `--no-install-targets`). There is no automatic
native cross-compilation: build the binary on the target OS or VM.

## Module map

| Module | Responsibility |
| --- | --- |
| `build/mod.rs` | Public build/packaging facade |
| `build/source.rs` | Extension root discovery, manifest reading, Cargo metadata |
| `build/compile.rs` | Per-target Cargo build, target installation, artifact paths |
| `build/package.rs` | ZIP assembly, entrypoint selection, and manifest rewriting |
| `build/paths.rs` | Package-relative path validation and ZIP name rules |
| `scaffold.rs` | `new`/`init` project generation |
| `target.rs` | Canonical target table and artifact naming |

See the [parent README](../README.md) for the manifest format, supported
targets, and include gating rules.
