use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{ItemFn, parse_macro_input};

#[derive(Clone, Copy)]
pub(crate) enum Phase {
    Preload,
    Unload,
}

pub(crate) fn expand_phase(item: TokenStream, phase: Phase) -> TokenStream {
    let function = parse_macro_input!(item as ItemFn);
    let phase_name = match phase {
        Phase::Preload => "preload",
        Phase::Unload => "unload",
    };
    if function.sig.asyncness.is_some() {
        return syn::Error::new_spanned(
            function.sig.fn_token,
            format!("notwig_ext({phase_name}) entrypoint must not be async"),
        )
        .to_compile_error()
        .into();
    }
    let entrypoint = &function.sig.ident;
    let module = format_ident!("__notwig_ext_{}_guest_{}", phase_name, entrypoint);
    let export = format_ident!("notwig_ext_{}", phase_name);
    let body = match phase {
        Phase::Preload => quote! {
            match notwig_ext_api::lifecycle::run_preload(|extension| {
                super::#entrypoint(extension);
            }) {
                Ok(()) => AbiStatus::Ok.as_i32(),
                Err(status) => status.as_i32(),
            }
        },
        Phase::Unload => quote! {
            notwig_ext_api::lifecycle::run_unload(|context| {
                super::#entrypoint(context);
            });
            AbiStatus::Ok.as_i32()
        },
    };
    quote! {
        #function

        #[doc(hidden)]
        #[allow(non_snake_case)]
        mod #module {
            use notwig_ext_api::AbiStatus;

            #[unsafe(no_mangle)]
            pub extern "C" fn #export() -> i32 {
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    #body
                })) {
                    Ok(status) => status,
                    Err(_) => AbiStatus::Error.as_i32(),
                }
            }
        }
    }
    .into()
}
