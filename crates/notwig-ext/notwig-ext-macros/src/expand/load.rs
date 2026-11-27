use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{ItemFn, parse_macro_input};

pub(crate) fn expand_load(item: TokenStream) -> TokenStream {
    let function = parse_macro_input!(item as ItemFn);
    if function.sig.asyncness.is_some() {
        return syn::Error::new_spanned(
            function.sig.fn_token,
            "notwig_ext(load) entrypoint must not be async",
        )
        .to_compile_error()
        .into();
    }
    let entrypoint = &function.sig.ident;
    let wasm_module = format_ident!("__notwig_ext_load_wasm_{}", entrypoint);
    let native_module = format_ident!("__notwig_ext_load_native_{}", entrypoint);
    let wasm_draw_exports = quote! {
        #[unsafe(no_mangle)]
        pub extern "C" fn notwig_device_draw(
            handle: i32,
            ctx_ptr: i32,
            ctx_len: i32,
        ) -> i64 {
            catch_value(0, || {
                with_bytes(ctx_ptr, ctx_len, |bytes| {
                    extension().draw_pointer(handle, bytes)
                })
                .unwrap_or(0)
            })
        }
    };
    let wasm_sub_editor_exports = quote! {
        #[unsafe(no_mangle)]
        pub extern "C" fn notwig_audio_sub_editor_create(
            renderer_ptr: i32,
            renderer_len: i32,
        ) -> i32 {
            catch_value(-1, || {
                with_str(renderer_ptr, renderer_len, |renderer| {
                    extension().create_audio_sub_editor(renderer)
                })
                .unwrap_or(-1)
            })
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn notwig_audio_sub_editor_destroy(handle: i32) {
            catch_unit(|| extension().destroy_audio_sub_editor(handle));
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn notwig_audio_sub_editor_draw(
            handle: i32,
            ctx_ptr: i32,
            ctx_len: i32,
        ) -> i64 {
            catch_value(0, || {
                with_bytes(ctx_ptr, ctx_len, |bytes| {
                    extension().draw_audio_sub_editor_pointer(handle, bytes)
                })
                .unwrap_or(0)
            })
        }
    };
    let native_draw_exports = quote! {
        #[unsafe(no_mangle)]
        pub extern "C" fn notwig_device_draw(
            handle: i32,
            ctx_ptr: *const u8,
            ctx_len: usize,
        ) -> NotwigNativeBytes {
            catch_value(NotwigNativeBytes::empty(), || {
                unsafe {
                    with_bytes_ptr(ctx_ptr, ctx_len, |bytes| {
                        extension()
                            .draw_bytes(handle, bytes)
                            .map_or_else(NotwigNativeBytes::empty, |(ptr, len)| {
                                NotwigNativeBytes { ptr, len }
                            })
                    })
                }
                .unwrap_or_else(NotwigNativeBytes::empty)
            })
        }
    };
    let wasm_command_exports = quote! {
        #[unsafe(no_mangle)]
        pub extern "C" fn notwig_command_execute(
            command_ptr: i32,
            command_len: i32,
            context_ptr: i32,
            context_len: i32,
        ) -> i64 {
            catch_value(0, || {
                with_str(command_ptr, command_len, |id| {
                    with_bytes(context_ptr, context_len, |context| {
                        extension().execute_command_pointer(id, context)
                    })
                    .unwrap_or(0)
                })
                .unwrap_or(0)
            })
        }
    };
    let native_sub_editor_exports = quote! {
        #[unsafe(no_mangle)]
        pub extern "C" fn notwig_audio_sub_editor_create(
            renderer_ptr: *const u8,
            renderer_len: usize,
        ) -> i32 {
            catch_value(-1, || {
                unsafe {
                    with_str_ptr(renderer_ptr, renderer_len, |renderer| {
                        extension().create_audio_sub_editor(renderer)
                    })
                }
                .unwrap_or(-1)
            })
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn notwig_audio_sub_editor_destroy(handle: i32) {
            catch_unit(|| extension().destroy_audio_sub_editor(handle));
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn notwig_audio_sub_editor_draw(
            handle: i32,
            ctx_ptr: *const u8,
            ctx_len: usize,
        ) -> NotwigNativeBytes {
            catch_value(NotwigNativeBytes::empty(), || {
                unsafe {
                    with_bytes_ptr(ctx_ptr, ctx_len, |bytes| {
                        extension()
                            .draw_audio_sub_editor_bytes(handle, bytes)
                            .map_or_else(NotwigNativeBytes::empty, |(ptr, len)| {
                                NotwigNativeBytes { ptr, len }
                            })
                    })
                }
                .unwrap_or_else(NotwigNativeBytes::empty)
            })
        }
    };
    let native_command_exports = quote! {
        #[unsafe(no_mangle)]
        pub extern "C" fn notwig_command_execute(
            command_ptr: *const u8,
            command_len: usize,
            context_ptr: *const u8,
            context_len: usize,
        ) -> NotwigNativeBytes {
            catch_value(NotwigNativeBytes::empty(), || {
                unsafe {
                    with_str_ptr(command_ptr, command_len, |id| {
                        with_bytes_ptr(context_ptr, context_len, |context| {
                            extension()
                                .execute_command_result(id, context)
                                .map_or_else(NotwigNativeBytes::empty, |(ptr, len)| {
                                    NotwigNativeBytes { ptr: ptr as *const u8, len }
                                })
                        })
                        .unwrap_or_else(NotwigNativeBytes::empty)
                    })
                }
                .unwrap_or_else(NotwigNativeBytes::empty)
            })
        }
    };
    let panic_guards = quote! {
        fn catch_status(f: impl FnOnce() -> i32) -> i32 {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
                Ok(status) => status,
                Err(_) => AbiStatus::Error.as_i32(),
            }
        }

        fn catch_value<T>(fallback: T, f: impl FnOnce() -> T) -> T {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
                Ok(value) => value,
                Err(_) => fallback,
            }
        }

        fn catch_unit(f: impl FnOnce()) {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        }
    };
    let expanded = quote! {
        #function

        #[cfg(target_arch = "wasm32")]
        #[doc(hidden)]
        #[allow(non_snake_case)]
        mod #wasm_module {
            #![allow(unsafe_code)]

            use notwig_ext_api::abi::{alloc, free, read_param_id, with_bytes, with_str};
            use notwig_ext_api::{ABI_VERSION, AbiStatus, Extension, ExtensionBuilder, NOTE_EVENT_BYTES};

            fn load_extension(extension: &mut ExtensionBuilder) {
                super::#entrypoint(extension);
            }

            fn extension() -> &'static Extension {
                notwig_ext_api::lifecycle::ensure_loaded(load_extension)
            }

            #panic_guards

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_ext_load() -> i32 {
                catch_status(|| match notwig_ext_api::lifecycle::run_load(load_extension) {
                    Ok(()) => AbiStatus::Ok.as_i32(),
                    Err(status) => status.as_i32(),
                })
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_ext_registrations() -> i64 {
                catch_value(0, || extension().registrations_pointer())
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_abi_version() -> i32 {
                i32::try_from(ABI_VERSION).unwrap_or(1)
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_alloc(len: i32) -> i32 {
                catch_value(-1, || alloc(len))
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_free(ptr: i32, len: i32) {
                catch_unit(|| free(ptr, len));
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_device_describe(type_ptr: i32, type_len: i32) -> i64 {
                catch_value(0, || {
                    with_str(type_ptr, type_len, |type_id| {
                        extension().describe_pointer(type_id)
                    })
                    .unwrap_or(0)
                })
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_device_create(type_ptr: i32, type_len: i32) -> i32 {
                catch_value(-1, || {
                    with_str(type_ptr, type_len, |type_id| {
                        extension().create_device(type_id)
                    })
                    .unwrap_or(-1)
                })
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_device_destroy(handle: i32) {
                catch_unit(|| extension().destroy_device(handle));
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_device_prepare(
                handle: i32,
                sample_rate: i32,
                _max_block: i32,
                channels: i32,
            ) -> i32 {
                catch_status(|| {
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
                })
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_device_set_param(
                handle: i32,
                param_ptr: i32,
                param_len: i32,
                _tag: i32,
                value: f64,
            ) -> i32 {
                catch_status(|| {
                    let Some(param_id) = read_param_id(param_ptr, param_len) else {
                        return AbiStatus::InvalidArgument.as_i32();
                    };
                    match extension().set_param(handle, param_id, value) {
                        Ok(true) => AbiStatus::Ok.as_i32(),
                        Ok(false) => AbiStatus::InvalidArgument.as_i32(),
                        Err(status) => status.as_i32(),
                    }
                })
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
                catch_status(|| {
                    let Ok(frames) = usize::try_from(frames) else {
                        return AbiStatus::InvalidArgument.as_i32();
                    };
                    if frames == 0 {
                        return AbiStatus::Ok.as_i32();
                    }
                    if event_count == 0 {
                        return match extension().process(handle, audio_ptr, frames, &[]) {
                            Ok(()) => AbiStatus::Ok.as_i32(),
                            Err(status) => status.as_i32(),
                        };
                    }
                    let events_len = match event_count.checked_mul(NOTE_EVENT_BYTES as i32) {
                        Some(len) => len,
                        None => return AbiStatus::InvalidArgument.as_i32(),
                    };
                    with_bytes(events_ptr, events_len, |events| {
                        match extension().process(handle, audio_ptr, frames, events) {
                            Ok(()) => AbiStatus::Ok.as_i32(),
                            Err(status) => status.as_i32(),
                        }
                    })
                    .unwrap_or(AbiStatus::InvalidArgument.as_i32())
                })
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_device_latency(handle: i32) -> i32 {
                catch_value(0, || {
                    i32::try_from(extension().latency(handle)).unwrap_or(0)
                })
            }
           #wasm_draw_exports
           #wasm_sub_editor_exports
           #wasm_command_exports
        }

        #[cfg(not(target_arch = "wasm32"))]
        #[doc(hidden)]
        #[allow(non_snake_case)]
        mod #native_module {
            #![allow(unsafe_code)]

            use notwig_ext_api::abi::{
                read_param_id_ptr, with_bytes_ptr, with_str_ptr,
            };
            use notwig_ext_api::{ABI_VERSION, AbiStatus, Extension, ExtensionBuilder, NOTE_EVENT_BYTES};

            fn load_extension(extension: &mut ExtensionBuilder) {
                super::#entrypoint(extension);
            }

            fn extension() -> &'static Extension {
                notwig_ext_api::lifecycle::ensure_loaded(load_extension)
            }

            #panic_guards

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_ext_load() -> i32 {
                catch_status(|| match notwig_ext_api::lifecycle::run_load(load_extension) {
                    Ok(()) => AbiStatus::Ok.as_i32(),
                    Err(status) => status.as_i32(),
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
            pub extern "C" fn notwig_ext_registrations() -> NotwigNativeBytes {
                catch_value(NotwigNativeBytes::empty(), || {
                    extension()
                        .registrations_result()
                        .map_or_else(NotwigNativeBytes::empty, |(ptr, len)| {
                            NotwigNativeBytes { ptr: ptr as *const u8, len }
                        })
                })
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
                catch_value(NotwigNativeBytes::empty(), || {
                    unsafe {
                        with_str_ptr(type_ptr, type_len, |type_id| {
                            extension().describe_bytes(type_id).map_or_else(
                                NotwigNativeBytes::empty,
                                NotwigNativeBytes::from_bytes,
                            )
                        })
                    }
                    .unwrap_or_else(NotwigNativeBytes::empty)
                })
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_device_create(
                type_ptr: *const u8,
                type_len: usize,
            ) -> i32 {
                catch_value(-1, || {
                    unsafe {
                        with_str_ptr(type_ptr, type_len, |type_id| {
                            extension().create_device(type_id)
                        })
                    }
                    .unwrap_or(-1)
                })
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_device_destroy(handle: i32) {
                catch_unit(|| extension().destroy_device(handle));
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_device_prepare(
                handle: i32,
                sample_rate: i32,
                _max_block: i32,
                channels: i32,
            ) -> i32 {
                catch_status(|| {
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
                })
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_device_set_param(
                handle: i32,
                param_ptr: *const u8,
                param_len: usize,
                _tag: i32,
                value: f64,
            ) -> i32 {
                catch_status(|| {
                    let Some(param_id) = (unsafe { read_param_id_ptr(param_ptr, param_len) }) else {
                        return AbiStatus::InvalidArgument.as_i32();
                    };
                    match extension().set_param(handle, param_id, value) {
                        Ok(true) => AbiStatus::Ok.as_i32(),
                        Ok(false) => AbiStatus::InvalidArgument.as_i32(),
                        Err(status) => status.as_i32(),
                    }
                })
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
                catch_status(|| {
                    let Ok(frames) = usize::try_from(frames) else {
                        return AbiStatus::InvalidArgument.as_i32();
                    };
                    if frames == 0 {
                        return AbiStatus::Ok.as_i32();
                    }
                    if event_count == 0 {
                        return match unsafe {
                            extension().process_ptr(handle, audio_ptr, frames, &[])
                        } {
                            Ok(()) => AbiStatus::Ok.as_i32(),
                            Err(status) => status.as_i32(),
                        };
                    }
                    let event_count = match usize::try_from(event_count) {
                        Ok(count) => count,
                        Err(_) => return AbiStatus::InvalidArgument.as_i32(),
                    };
                    let events_len = match event_count.checked_mul(NOTE_EVENT_BYTES) {
                        Some(len) => len,
                        None => return AbiStatus::InvalidArgument.as_i32(),
                    };
                    unsafe {
                        with_bytes_ptr(events_ptr, events_len, |events| {
                            match extension().process_ptr(handle, audio_ptr, frames, events) {
                                Ok(()) => AbiStatus::Ok.as_i32(),
                                Err(status) => status.as_i32(),
                            }
                        })
                    }
                    .unwrap_or(AbiStatus::InvalidArgument.as_i32())
                })
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn notwig_device_latency(handle: i32) -> i32 {
                catch_value(0, || {
                    i32::try_from(extension().latency(handle)).unwrap_or(0)
                })
            }
           #native_draw_exports
           #native_sub_editor_exports
           #native_command_exports
        }
    };
    expanded.into()
}
