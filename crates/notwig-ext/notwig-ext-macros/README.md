# notwig-ext-macros

Proc-macro support for Notwig WASM and native extensions.

`#[notwig_extension]` wraps an entrypoint function and generates the ABI exports
the host expects:

```rust
use notwig_ext_api::{Extension, notwig_extension};

#[notwig_extension]
pub fn extension(extension: &mut Extension) {
    extension.register_device::<MyDevice>();
}
```

The entrypoint runs once, lazily, when the host first calls an ABI export. On
`wasm32-unknown-unknown` the macro generates `notwig_abi_version`,
`notwig_alloc`, `notwig_free`, and the `notwig_device_*` family. On native
targets it generates the same version and device symbol names with
pointer-sized arguments and direct host pointers. All generated functions
forward to `notwig_ext_api::Extension`; extensions normally only implement
`DeviceExtension` and register the type.

Each compiled extension module must contain exactly one `#[notwig_extension]`
entrypoint; the generated exports have fixed names.
