# notwig-ext-macros

Proc-macro support for Notwig WASM extensions.

`#[notwig_extension]` wraps an entrypoint function and generates the ABI exports
the host expects:

```rust
use notwig_ext_api::{Extension, notwig_extension};

#[notwig_extension]
pub fn extension(extension: &mut Extension) {
    extension.register_device::<MyDevice>();
}
```

The entrypoint runs once, lazily, when the host first calls an ABI export. The
macro generates `notwig_abi_version`, `notwig_alloc`, `notwig_free`, and the
`notwig_device_*` family. All generated functions forward to
`notwig_ext_api::Extension`; extensions normally only implement
`DeviceExtension` and register the type.

Each wasm module must contain exactly one `#[notwig_extension]` entrypoint; the
generated exports have fixed names.
