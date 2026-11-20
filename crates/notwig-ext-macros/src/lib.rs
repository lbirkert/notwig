#![forbid(unsafe_code)]

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{ItemFn, parse_macro_input};

/// Generates the Notwig wasm extension ABI around an entrypoint function.
///
/// ```ignore
/// #[notwig_extension]
/// pub fn extension(extension: &mut notwig_ext_api::Extension) {
///     extension.register_device::<MyDevice>();
/// }
/// ```
///
/// The entrypoint runs once, lazily, when the host first calls an ABI export.
/// Each wasm module must contain exactly one `#[notwig_extension]` entrypoint.
#[proc_macro_attribute]
pub fn notwig_extension(_attr: TokenStream, item: TokenStream) -> TokenStream {
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
    let abi_module = format_ident!("__notwig_extension_abi_{}", entrypoint);
    let expanded = quote! {
        #function

        #[doc(hidden)]
        #[allow(non_snake_case)]
        mod #abi_module {
            #![allow(unsafe_code)]

            use std::sync::{Mutex, MutexGuard, OnceLock};

            use notwig_ext_api::abi::{alloc, free, read_param_id, with_bytes, with_str};
            use notwig_ext_api::{ABI_VERSION, AbiStatus, Extension, NOTE_EVENT_BYTES};

            static EXTENSION: OnceLock<Mutex<Extension>> = OnceLock::new();

            fn extension_state() -> MutexGuard<'static, Extension> {
                EXTENSION
                    .get_or_init(|| {
                        let mut extension = Extension::new();
                        super::#entrypoint(&mut extension);
                        Mutex::new(extension)
                    })
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
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
                    extension_state().describe_pointer(type_id)
                })
                .unwrap_or(0)
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_device_create(type_ptr: i32, type_len: i32) -> i32 {
                with_str(type_ptr, type_len, |type_id| {
                    extension_state().create_device(type_id)
                })
                .unwrap_or(-1)
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_device_destroy(handle: i32) {
                extension_state().destroy_device(handle);
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
                match extension_state().prepare(handle, sample_rate, channels) {
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
                match extension_state().set_param(handle, param_id, value) {
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
                    match extension_state().process(handle, audio_ptr, frames, &[]) {
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
                        match extension_state().process(handle, audio_ptr, frames, events) {
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
                i32::try_from(extension_state().latency(handle)).unwrap_or(0)
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_device_draw(
                handle: i32,
                ctx_ptr: i32,
                ctx_len: i32,
            ) -> i64 {
                with_bytes(ctx_ptr, ctx_len, |bytes| {
                    extension_state().draw_pointer(handle, bytes)
                })
                .unwrap_or(0)
            }
        }
    };
    expanded.into()
}
