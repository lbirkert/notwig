//! Procedural macros for defining Notwig extension entrypoints.
//!
//! This crate provides [`notwig_extension`], which wraps a single registration
//! function with the generated WASM or native ABI exports. It is re-exported by
//! `notwig-ext-api`; extension authors normally use that re-export.

#![forbid(unsafe_code)]

use proc_macro::TokenStream;

mod expand;
mod trait_macro;

use expand::{Phase, expand_load, expand_phase};

/// Generates the Notwig extension ABI around a load-phase entrypoint.
///
/// This is the legacy single-entrypoint attribute. New extensions should use
/// [`macro@notwig_ext`] with explicit phases.
///
/// ```ignore
/// #[notwig_extension]
/// pub fn extension(extension: &mut notwig_ext_api::ExtensionBuilder) {
///     extension.register_extension::<MyDevice>(notwig_ext_api::EP_NOTWIG_DEVICES);
/// }
/// ```
///
/// The entrypoint runs once, lazily, when the host first calls an ABI export.
/// Each extension must contain exactly one load-phase entrypoint.
///
/// The macro emits a WASM ABI when compiled for `wasm32-unknown-unknown` and a
/// native shared-library ABI for every other target. Native exports pass
/// ordinary process pointers, so audio is processed in the host's buffer
/// without a copy.
///
/// # Entrypoint signature
///
/// The annotated function must be a non-`async` function taking exactly one
/// `&mut notwig_ext_api::ExtensionBuilder` argument. A duplicate annotation in
/// one crate produces conflicting `notwig_*` exports at link time.
///
/// # Generated exports
///
/// Both ABIs export `notwig_abi_version`, `notwig_ext_load`,
/// `notwig_ext_registrations`,
/// `notwig_device_describe`, `notwig_device_create`, `notwig_device_destroy`,
/// `notwig_device_prepare`, `notwig_device_set_param`, `notwig_device_process`,
/// `notwig_device_latency`, `notwig_device_draw`,
/// `notwig_audio_sub_editor_create`, `notwig_audio_sub_editor_destroy`, and
/// `notwig_audio_sub_editor_draw`. The WASM build additionally exports
/// `notwig_alloc` and `notwig_free` for the host scratch heap.
///
/// # ABI and panics
///
/// The host requires an exact `notwig_ext_api::ABI_VERSION` match. The generated
/// exports wrap recoverable bodies in `std::panic::catch_unwind`: status calls
/// return `AbiStatus::Error`, pointer-returning calls return null/empty, and
/// `destroy` swallows the panic. This only helps builds that unwind; a
/// `panic = "abort"` WASM module or native library still aborts, and a WASM trap
/// cannot be recovered by the host. Return `notwig_ext_api::AbiStatus` errors
/// instead of panicking for recoverable failures.
#[proc_macro_attribute]
pub fn notwig_extension(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "notwig_extension takes no options; use notwig_ext(load) for the phased form",
        )
        .to_compile_error()
        .into();
    }
    expand_load(item)
}

/// Marks one extension lifecycle phase.
///
/// Supported forms are `#[notwig_ext(preload)]`, `#[notwig_ext(load)]`, and
/// `#[notwig_ext(unload)]`. One binary may define at most one function per
/// phase. Preload declares extension points, load registers implementations and
/// contributions, and unload releases extension-owned resources.
///
/// `load` generates the main extension ABI and is required for packages that
/// expose devices, commands, or sub-editors. The other phases are optional.
#[proc_macro_attribute]
pub fn notwig_ext(attr: TokenStream, item: TokenStream) -> TokenStream {
    let phase = attr.to_string();
    match phase.trim() {
        "load" => expand_load(item),
        "preload" => expand_phase(item, Phase::Preload),
        "unload" => expand_phase(item, Phase::Unload),
        _ => syn::Error::new(
            proc_macro2::Span::call_site(),
            "notwig_ext expects preload, load, or unload",
        )
        .to_compile_error()
        .into(),
    }
}

/// Generates an RPC handle, wire request/response types, and a dispatcher for a trait.
///
/// ```ignore
/// #[notwig_ext_trait(id = "com.example.echo", version = 1)]
/// pub trait Echo {
///     fn echo(&self, value: String) -> Result<String, notwig_ext_api::ExtensionError>;
/// }
/// ```
///
/// Methods must take `&self`, have no generics, and return
/// `Result<T, ExtensionError>`. Arguments and return values must be
/// postcard-serializable.
#[proc_macro_attribute]
pub fn notwig_ext_trait(attr: TokenStream, item: TokenStream) -> TokenStream {
    trait_macro::expand(attr, item)
}
