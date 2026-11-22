#![forbid(unsafe_code)]

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{ItemFn, parse_macro_input};

/// Generates the Notwig extension ABI around an entrypoint function.
///
/// ```ignore
/// #[notwig_extension]
/// pub fn extension(extension: &mut notwig_ext_api::Extension) {
///     extension.register_device::<MyDevice>();
/// }
/// ```
///
/// The entrypoint runs once, lazily, when the host first calls an ABI export.
/// Each extension must contain exactly one `#[notwig_extension]` entrypoint.
///
/// The macro emits a WASM ABI when compiled for `wasm32-unknown-unknown` and a
/// native shared-library ABI for every other target. Native exports pass
/// ordinary process pointers, so audio is processed in the host's buffer
/// without a copy.
#[proc_macro_attribute]
pub fn notwig_extension(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        let supported = attr
            .to_string()
            .split(',')
            .all(|part| part.trim() == "egui");
        if !supported {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                "unsupported notwig_extension option",
            )
            .to_compile_error()
            .into();
        }
    }
    let function = parse_macro_input!(item as ItemFn);
    if function.sig.asyncness.is_some() {
        return syn::Error::new_spanned(
            function.sig.fn_token,
            "notwig_extension entrypoint must not be async",
        )
        .to_compile_error()
        .into();
    }
    let entrypoint = &function.sig.ident;
    let wasm_module = format_ident!("__notwig_extension_wasm_{}", entrypoint);
    let native_module = format_ident!("__notwig_extension_native_{}", entrypoint);
    let wasm_egui_exports = quote! {
        #[unsafe(no_mangle)]
        pub extern "C" fn notwig_device_draw_egui(
            handle: i32,
            ctx_ptr: i32,
            ctx_len: i32,
        ) -> i64 {
            with_bytes(ctx_ptr, ctx_len, |bytes| {
                extension().draw_egui_pointer(handle, bytes)
            })
            .unwrap_or(0)
        }
    };
    let native_egui_exports = quote! {
        #[unsafe(no_mangle)]
        pub extern "C" fn notwig_device_draw_egui(
            handle: i32,
            ctx_ptr: *const u8,
            ctx_len: usize,
        ) -> NotwigNativeBytes {
            with_bytes_ptr(ctx_ptr, ctx_len, |bytes| {
                extension()
                    .draw_egui_bytes(handle, bytes)
                    .map_or_else(NotwigNativeBytes::empty, |(ptr, len)| {
                        NotwigNativeBytes { ptr, len }
                    })
            })
            .unwrap_or_else(NotwigNativeBytes::empty)
        }
    };
    let expanded = quote! {
        #function

        #[cfg(target_arch = "wasm32")]
        #[doc(hidden)]
        #[allow(non_snake_case)]
        mod #wasm_module {
            #![allow(unsafe_code)]

            use std::sync::OnceLock;

            use notwig_ext_api::abi::{alloc, free, read_param_id, with_bytes, with_str};
            use notwig_ext_api::{ABI_VERSION, AbiStatus, Extension, NOTE_EVENT_BYTES};

            static EXTENSION: OnceLock<Extension> = OnceLock::new();

            fn extension() -> &'static Extension {
                EXTENSION.get_or_init(|| {
                    let mut extension = Extension::new();
                    super::#entrypoint(&mut extension);
                    extension
                })
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_abi_version() -> i32 {
                i32::try_from(ABI_VERSION).unwrap_or(1)
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_alloc(len: i32) -> i32 {
                alloc(len)
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_free(ptr: i32, len: i32) {
                free(ptr, len);
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_device_describe(type_ptr: i32, type_len: i32) -> i64 {
                with_str(type_ptr, type_len, |type_id| {
                    extension().describe_pointer(type_id)
                })
                .unwrap_or(0)
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_device_create(type_ptr: i32, type_len: i32) -> i32 {
                with_str(type_ptr, type_len, |type_id| {
                    extension().create_device(type_id)
                })
                .unwrap_or(-1)
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_device_destroy(handle: i32) {
                extension().destroy_device(handle);
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_device_prepare(
                handle: i32,
                sample_rate: i32,
                _max_block: i32,
                channels: i32,
            ) -> i32 {
                let Ok(sample_rate) = u32::try_from(sample_rate) else {
                    return AbiStatus::InvalidArgument.as_i32();
                };
                let Ok(channels) = u16::try_from(channels) else {
                    return AbiStatus::InvalidArgument.as_i32();
                };
                match extension().prepare(handle, sample_rate, channels) {
                    Ok(()) => AbiStatus::Ok.as_i32(),
                    Err(status) => status.as_i32(),
                }
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_device_set_param(
                handle: i32,
                param_ptr: i32,
                param_len: i32,
                _tag: i32,
                value: f64,
            ) -> i32 {
                let Some(param_id) = read_param_id(param_ptr, param_len) else {
                    return AbiStatus::InvalidArgument.as_i32();
                };
                match extension().set_param(handle, param_id, value) {
                    Ok(true) => AbiStatus::Ok.as_i32(),
                    Ok(false) => AbiStatus::InvalidArgument.as_i32(),
                    Err(status) => status.as_i32(),
                }
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_device_process(
                handle: i32,
                audio_ptr: i32,
                frames: i32,
                events_ptr: i32,
                event_count: i32,
                _sample_position: i64,
                _playing: i32,
            ) -> i32 {
                let Ok(frames) = usize::try_from(frames) else {
                    return AbiStatus::InvalidArgument.as_i32();
                };
                if frames == 0 {
                    return AbiStatus::Ok.as_i32();
                }
                let status = if event_count == 0 {
                    match extension().process(handle, audio_ptr, frames, &[]) {
                        Ok(()) => AbiStatus::Ok,
                        Err(status) => status,
                    }
                } else {
                    let Some(events_len) =
                        event_count.checked_mul(NOTE_EVENT_BYTES as i32)
                    else {
                        return AbiStatus::InvalidArgument.as_i32();
                    };
                    with_bytes(events_ptr, events_len, |events| {
                        match extension().process(handle, audio_ptr, frames, events) {
                            Ok(()) => AbiStatus::Ok,
                            Err(status) => status,
                        }
                    })
                    .unwrap_or(AbiStatus::InvalidArgument)
                };
                status.as_i32()
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_device_latency(handle: i32) -> i32 {
                i32::try_from(extension().latency(handle)).unwrap_or(0)
            }
           #wasm_egui_exports
        }

        #[cfg(not(target_arch = "wasm32"))]
        #[doc(hidden)]
        #[allow(non_snake_case)]
        mod #native_module {
            #![allow(unsafe_code)]

            use std::sync::OnceLock;

            use notwig_ext_api::abi::{
                read_param_id_ptr, with_bytes_ptr, with_str_ptr,
            };
            use notwig_ext_api::{ABI_VERSION, AbiStatus, Extension, NOTE_EVENT_BYTES};

            static EXTENSION: OnceLock<Extension> = OnceLock::new();

            fn extension() -> &'static Extension {
                EXTENSION.get_or_init(|| {
                    let mut extension = Extension::new();
                    super::#entrypoint(&mut extension);
                    extension
                })
            }

            /// Two-word pointer/length result shared with the native host.
            #[repr(C)]
            pub struct NotwigNativeBytes {
                pub ptr: *const u8,
                pub len: usize,
            }

            impl NotwigNativeBytes {
                fn empty() -> Self {
                    Self {
                        ptr: std::ptr::null(),
                        len: 0,
                    }
                }

                fn from_bytes(bytes: &[u8]) -> Self {
                    Self {
                        ptr: bytes.as_ptr(),
                        len: bytes.len(),
                    }
                }
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_abi_version() -> i32 {
                i32::try_from(ABI_VERSION).unwrap_or(1)
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_device_describe(
                type_ptr: *const u8,
                type_len: usize,
            ) -> NotwigNativeBytes {
                with_str_ptr(type_ptr, type_len, |type_id| {
                    extension()
                        .describe_bytes(type_id)
                        .map_or_else(NotwigNativeBytes::empty, NotwigNativeBytes::from_bytes)
                })
                .unwrap_or_else(NotwigNativeBytes::empty)
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_device_create(
                type_ptr: *const u8,
                type_len: usize,
            ) -> i32 {
                with_str_ptr(type_ptr, type_len, |type_id| {
                    extension().create_device(type_id)
                })
                .unwrap_or(-1)
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_device_destroy(handle: i32) {
                extension().destroy_device(handle);
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_device_prepare(
                handle: i32,
                sample_rate: i32,
                _max_block: i32,
                channels: i32,
            ) -> i32 {
                let Ok(sample_rate) = u32::try_from(sample_rate) else {
                    return AbiStatus::InvalidArgument.as_i32();
                };
                let Ok(channels) = u16::try_from(channels) else {
                    return AbiStatus::InvalidArgument.as_i32();
                };
                match extension().prepare(handle, sample_rate, channels) {
                    Ok(()) => AbiStatus::Ok.as_i32(),
                    Err(status) => status.as_i32(),
                }
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_device_set_param(
                handle: i32,
                param_ptr: *const u8,
                param_len: usize,
                _tag: i32,
                value: f64,
            ) -> i32 {
                let Some(param_id) = read_param_id_ptr(param_ptr, param_len) else {
                    return AbiStatus::InvalidArgument.as_i32();
                };
                match extension().set_param(handle, param_id, value) {
                    Ok(true) => AbiStatus::Ok.as_i32(),
                    Ok(false) => AbiStatus::InvalidArgument.as_i32(),
                    Err(status) => status.as_i32(),
                }
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_device_process(
                handle: i32,
                audio_ptr: *mut f32,
                frames: i32,
                events_ptr: *const u8,
                event_count: i32,
                _sample_position: i64,
                _playing: i32,
            ) -> i32 {
                let Ok(frames) = usize::try_from(frames) else {
                    return AbiStatus::InvalidArgument.as_i32();
                };
                if frames == 0 {
                    return AbiStatus::Ok.as_i32();
                }
                let status = if event_count == 0 {
                    match extension().process_ptr(handle, audio_ptr, frames, &[]) {
                        Ok(()) => AbiStatus::Ok,
                        Err(status) => status,
                    }
                } else {
                    let Ok(event_count) = usize::try_from(event_count) else {
                        return AbiStatus::InvalidArgument.as_i32();
                    };
                    let Some(events_len) = event_count.checked_mul(NOTE_EVENT_BYTES) else {
                        return AbiStatus::InvalidArgument.as_i32();
                    };
                    with_bytes_ptr(events_ptr, events_len, |events| {
                        match extension().process_ptr(handle, audio_ptr, frames, events) {
                            Ok(()) => AbiStatus::Ok,
                            Err(status) => status,
                        }
                    })
                    .unwrap_or(AbiStatus::InvalidArgument)
                };
                status.as_i32()
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_device_latency(handle: i32) -> i32 {
                i32::try_from(extension().latency(handle)).unwrap_or(0)
            }
           #native_egui_exports
        }
    };
    expanded.into()
}
