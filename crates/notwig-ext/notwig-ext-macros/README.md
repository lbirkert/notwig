# notwig-ext-macros

Proc-macro support for Notwig WASM and native extensions.

`#[notwig_ext(load)]` wraps the load-phase entrypoint and generates the main ABI
exports the host expects:

```rust
use notwig_ext_api::{ExtensionBuilder, notwig_ext};

#[notwig_ext(load)]
pub fn load(extension: &mut ExtensionBuilder) {
    extension.register_device::<MyDevice>();
}
```

Two optional phases run around it:

```rust
use notwig_ext_api::{ExtensionPoint, PreloadBuilder, UnloadContext, notwig_ext};

const MY_POINT: ExtensionPoint<dyn MyInterface> =
    ExtensionPoint::new("com.example.my-point");

#[notwig_ext(preload)]
pub fn preload(extension: &mut PreloadBuilder) {
    extension.register_extension_point(MY_POINT);
}

#[notwig_ext(unload)]
pub fn unload(_context: &mut UnloadContext) {}
```

Preload declarations are collected before any load-phase registration. Unload
runs when the host drops the extension instance. The legacy
`#[notwig_extension]` attribute is kept as an alias for `#[notwig_ext(load)]`.

The load entrypoint runs once, lazily, when the host first calls an ABI export
or explicitly calls `notwig_ext_load`. On `wasm32-unknown-unknown` the macro
generates `notwig_abi_version`, `notwig_ext_load`,
`notwig_ext_registrations`, `notwig_alloc`, `notwig_free`, and the
`notwig_device_*` family. On native targets it generates
the same version, phase, and device symbol names with pointer-sized arguments
and direct host pointers. All generated functions forward to
`notwig_ext_api::Extension`; extensions normally only implement
`DeviceExtension` and register the type.

Each compiled extension module must contain at most one function per phase and
exactly one `#[notwig_ext(load)]` entrypoint; the generated exports have fixed
names.
