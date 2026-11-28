# notwig-ext-macros

Proc-macro support for Notwig WASM and native extensions.

`#[notwig_ext(load)]` wraps the load-phase entrypoint and generates the main ABI
exports the host expects:

```rust
use notwig_ext_api::{EP_NOTWIG_DEVICES, ExtensionBuilder, notwig_ext};

#[notwig_ext(load)]
pub fn load(extension: &mut ExtensionBuilder) {
    extension.register_extension::<MyDevice>(EP_NOTWIG_DEVICES);
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

## Typed interfaces

`#[notwig_ext_trait(id = "...", version = N)]` generates a typed RPC handle,
request/response enums, and a dispatcher for an extension interface:

```rust
#[notwig_ext_trait(id = "com.example.echo", version = 1)]
pub trait Echo {
    fn echo(
        &self,
        context: &notwig_ext_api::ExtensionContext<'_>,
        value: String,
    ) -> Result<String, notwig_ext_api::ExtensionError>;
}
```

Methods must take `&self` followed by `&ExtensionContext<'_>`, have no
generics/lifetimes, and return `Result<T, ExtensionError>`. The context is how a
handler calls other extension points. Arguments and return values are bitcode
encoded. The generated handle takes an [`RpcTransport`] and `RemoteTarget`; the
generated `EchoDispatcher` forwards the context and is implemented for every
`Echo + Send + Sync` implementation. The macro also generates
`ExtensionHandle` for `dyn Trait`, which exposes the interface metadata and
associates the generated `TraitHandle` as `type Handle`. A scheduler-backed
`ExtensionContext` can create the handle with `get_extension_handle`. Raw-request
processing lives on `ExtensionCall`.

Annotate the concrete implementation so the instance can be stored as
`Box<dyn ExtensionCall>` without moving it across the ABI. The package invoke
export is not wired yet; this storage and dispatch path is used by host/core
and in-process callers today:

```rust
#[derive(ExtensionCall)]
#[extension_call(Echo)]
struct MyEcho;

#[notwig_ext(load)]
pub fn load(extension: &mut ExtensionBuilder) {
    extension.register_extension_instance(ECHO_POINT, MyEcho::new());
}
```

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
