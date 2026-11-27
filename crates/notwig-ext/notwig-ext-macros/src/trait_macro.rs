use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{format_ident, quote};
use syn::{
    FnArg, GenericArgument, ItemTrait, LitStr, Pat, PatType, PathArguments, ReturnType, TraitItem,
    TraitItemFn, Type,
};

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut id = None;
    let mut version = 1_u32;
    let parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("id") {
            let value: LitStr = meta.value()?.parse()?;
            id = Some(value);
            Ok(())
        } else if meta.path.is_ident("version") {
            let value: syn::LitInt = meta.value()?.parse()?;
            version = value.base10_parse()?;
            Ok(())
        } else {
            Err(meta.error("expected id = \"...\" and optional version = N"))
        }
    });
    syn::parse_macro_input!(attr with parser);
    let item: ItemTrait = syn::parse_macro_input!(item as ItemTrait);
    let Some(id) = id else {
        return syn::Error::new(Span::call_site(), "notwig_ext_trait requires id = \"...\"")
            .to_compile_error()
            .into();
    };
    let trait_ident = &item.ident;
    let handle_ident = format_ident!("{}Handle", trait_ident);
    let request_ident = format_ident!("{}Request", trait_ident);
    let response_ident = format_ident!("{}Response", trait_ident);
    let dispatcher_ident = format_ident!("{}Dispatcher", trait_ident);
    let id_lit = &id;

    let mut request_variants = Vec::new();
    let mut response_variants = Vec::new();
    let mut handle_methods = Vec::new();
    let mut dispatch_arms = Vec::new();
    let mut method_names = Vec::new();

    for trait_item in &item.items {
        let TraitItem::Fn(method) = trait_item else {
            return syn::Error::new_spanned(
                trait_item,
                "notwig_ext_trait supports functions only; associated types and consts are not allowed",
            )
            .to_compile_error()
            .into();
        };
        match parse_method(method, trait_ident) {
            Ok(parsed) => {
                let Method {
                    ident,
                    receiver,
                    context_pat,
                    context_ty,
                    args,
                    ret,
                } = parsed;
                let method_name = ident.to_string();
                let request_fields = args.iter().map(|(name, ty)| quote!(#name: #ty));
                let request_variant = if args.is_empty() {
                    quote!(#ident)
                } else {
                    quote!(#ident { #(#request_fields),* })
                };
                request_variants.push(request_variant);
                response_variants.push(
                    quote!(#ident(::core::result::Result<#ret, ::notwig_ext_api::ExtensionError>)),
                );

                let arg_decls = args.iter().map(|(name, ty)| quote!(#name: #ty));
                let request_init = if args.is_empty() {
                    quote!(#request_ident::#ident)
                } else {
                    let fields = args.iter().map(|(name, _)| quote!(#name));
                    quote!(#request_ident::#ident { #(#fields),* })
                };
                let response_pattern = if args.is_empty() {
                    quote!(#request_ident::#ident)
                } else {
                    let fields = args.iter().map(|(name, _)| quote!(#name));
                    quote!(#request_ident::#ident { #(#fields),* })
                };
                handle_methods.push(quote! {
                    fn #ident(#receiver, #context_pat: #context_ty #(, #arg_decls)*) -> ::core::result::Result<#ret, ::notwig_ext_api::ExtensionError> {
                        let request = #request_init;
                        let payload = ::notwig_ext_api::postcard::to_allocvec(&request)
                            .map_err(|error| ::notwig_ext_api::ExtensionError::new(
                                format!("encode {} request: {error}", #method_name)
                            ))?;
                        let response = self.transport.invoke(
                            &self.target,
                            #context_pat,
                            #id_lit,
                            #version,
                            #method_name,
                            &payload,
                        )?;
                        let response: #response_ident = ::notwig_ext_api::postcard::from_bytes(&response)
                            .map_err(|error| ::notwig_ext_api::ExtensionError::new(
                                format!("decode {} response: {error}", #method_name)
                            ))?;
                        match response {
                            #response_ident::#ident(result) => result,
                            _ => Err(::notwig_ext_api::ExtensionError::new("response method mismatch")),
                        }
                    }
                });
                let args_bindings = args.iter().map(|(name, _)| quote!(#name));
                dispatch_arms.push(quote! {
                    #method_name => {
                        let request: #request_ident = ::notwig_ext_api::postcard::from_bytes(payload)
                            .map_err(|error| ::notwig_ext_api::ExtensionError::new(
                                format!("decode {} request: {error}", #method_name)
                            ))?;
                        match request {
                            #response_pattern => {
                                let result = #trait_ident::#ident(self, context #(, #args_bindings)*);
                                ::notwig_ext_api::postcard::to_allocvec(&#response_ident::#ident(result))
                                    .map_err(|error| ::notwig_ext_api::ExtensionError::new(
                                        format!("encode {} response: {error}", #method_name)
                                    ))
                            }
                            _ => Err(::notwig_ext_api::ExtensionError::new("method payload mismatch")),
                        }
                    }
                });
                method_names.push(method_name);
            }
            Err(error) => return error.to_compile_error().into(),
        }
    }

    let expanded = quote! {
        #item

        /// Wire request for a generated extension interface.
        #[derive(::notwig_ext_api::serde::Serialize, ::notwig_ext_api::serde::Deserialize)]
        #[allow(non_camel_case_types)]
        pub enum #request_ident {
            #(#request_variants),*
        }

        /// Wire response for a generated extension interface.
        #[derive(::notwig_ext_api::serde::Serialize, ::notwig_ext_api::serde::Deserialize)]
        #[allow(non_camel_case_types)]
        pub enum #response_ident {
            #(#response_variants),*
        }

        /// Generated proxy that implements the annotated trait via RPC.
        #[derive(Clone)]
        pub struct #handle_ident {
            transport: ::notwig_ext_api::SharedRpcTransport,
            target: ::notwig_ext_api::RemoteTarget,
        }

        impl #handle_ident {
            /// Creates a handle for one remote implementation.
            #[must_use]
            pub fn new(
                transport: ::notwig_ext_api::SharedRpcTransport,
                target: ::notwig_ext_api::RemoteTarget,
            ) -> Self {
                Self { transport, target }
            }

            /// Interface id carried by this handle.
            pub const INTERFACE_ID: &'static str = #id_lit;
            /// Interface major version carried by this handle.
            pub const INTERFACE_VERSION: u32 = #version;
            /// Stable method ids in declaration order.
            pub const METHOD_IDS: &'static [&'static str] = &[#(#method_names),*];
        }

        impl #trait_ident for #handle_ident {
            #(#handle_methods)*
        }

        /// Server-side dispatcher for a generated extension interface.
        pub trait #dispatcher_ident: Send + Sync {
            /// Dispatches one postcard-encoded method call.
            fn dispatch_extension(
                &self,
                context: &::notwig_ext_api::ExtensionContext<'_>,
                method: &str,
                payload: &[u8],
            ) -> ::core::result::Result<Vec<u8>, ::notwig_ext_api::ExtensionError>;
        }

        impl<T: #trait_ident + Send + Sync + 'static> #dispatcher_ident for T {
            fn dispatch_extension(
                &self,
                context: &::notwig_ext_api::ExtensionContext<'_>,
                method: &str,
                payload: &[u8],
            ) -> ::core::result::Result<Vec<u8>, ::notwig_ext_api::ExtensionError> {
                match method {
                    #(#dispatch_arms),*,
                    _ => Err(::notwig_ext_api::ExtensionError::new(
                        format!("unknown {} method: {method}", #id_lit)
                    )),
                }
            }
        }
    };
    expanded.into()
}

struct Method {
    ident: syn::Ident,
    receiver: syn::Receiver,
    context_pat: syn::Ident,
    context_ty: Type,
    args: Vec<(syn::Ident, Type)>,
    ret: Type,
}

fn parse_method(method: &TraitItemFn, trait_ident: &syn::Ident) -> syn::Result<Method> {
    let sig = &method.sig;
    if sig.asyncness.is_some() {
        return Err(syn::Error::new_spanned(
            sig.fn_token,
            "async trait methods are not supported",
        ));
    }
    if sig.unsafety.is_some() || sig.abi.is_some() {
        return Err(syn::Error::new_spanned(
            sig,
            "unsafe and extern trait methods are not supported",
        ));
    }
    if !sig.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &sig.generics,
            "generic trait methods are not supported",
        ));
    }
    let mut inputs = sig.inputs.iter();
    let Some(FnArg::Receiver(receiver)) = inputs.next() else {
        return Err(syn::Error::new_spanned(
            sig,
            "trait methods must take &self as the first argument",
        ));
    };
    if receiver.reference.is_none() || receiver.mutability.is_some() {
        return Err(syn::Error::new_spanned(
            receiver,
            "only &self receivers are supported",
        ));
    }
    let Some(FnArg::Typed(context)) = inputs.next() else {
        return Err(syn::Error::new_spanned(
            sig,
            "trait methods must take &ExtensionContext<'_> after &self",
        ));
    };
    if !is_extension_context(context.ty.as_ref()) {
        return Err(syn::Error::new_spanned(
            context.ty.as_ref(),
            "the second argument must be &ExtensionContext<'_>",
        ));
    }
    let Pat::Ident(context_pat) = context.pat.as_ref() else {
        return Err(syn::Error::new_spanned(
            context.pat.as_ref(),
            "context argument must be a simple identifier",
        ));
    };
    let context_pat = context_pat.ident.clone();
    let context_ty = context.ty.as_ref().clone();
    let mut args = Vec::new();
    for input in inputs {
        let FnArg::Typed(PatType { pat, ty, .. }) = input else {
            return Err(syn::Error::new_spanned(input, "unsupported argument"));
        };
        let Pat::Ident(pat) = pat.as_ref() else {
            return Err(syn::Error::new_spanned(
                pat,
                "arguments must be simple identifiers",
            ));
        };
        args.push((pat.ident.clone(), ty.as_ref().clone()));
    }
    let ret = result_type(&sig.output, trait_ident)?;
    Ok(Method {
        ident: sig.ident.clone(),
        receiver: receiver.clone(),
        context_pat,
        context_ty,
        args,
        ret,
    })
}

fn is_extension_context(ty: &Type) -> bool {
    let Type::Reference(reference) = ty else {
        return false;
    };
    let Type::Path(path) = reference.elem.as_ref() else {
        return false;
    };
    path.path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "ExtensionContext")
}

fn result_type(output: &ReturnType, trait_ident: &syn::Ident) -> syn::Result<Type> {
    let ReturnType::Type(_, ty) = output else {
        return Err(syn::Error::new_spanned(
            output,
            "trait methods must return Result<T, ExtensionError>",
        ));
    };
    let Type::Path(path) = ty.as_ref() else {
        return Err(syn::Error::new_spanned(ty, "unsupported return type"));
    };
    let Some(segment) = path.path.segments.last() else {
        return Err(syn::Error::new_spanned(ty, "unsupported return type"));
    };
    if segment.ident != "Result" {
        return Err(syn::Error::new_spanned(
            ty,
            "trait methods must return Result",
        ));
    }
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return Err(syn::Error::new_spanned(
            ty,
            "Result must have two type arguments",
        ));
    };
    let mut types = arguments.args.iter().filter_map(|argument| match argument {
        GenericArgument::Type(ty) => Some(ty.clone()),
        _ => None,
    });
    let Some(value) = types.next() else {
        return Err(syn::Error::new_spanned(ty, "missing Result value type"));
    };
    let Some(error) = types.next() else {
        return Err(syn::Error::new_spanned(ty, "missing Result error type"));
    };
    let error_ok = match error {
        Type::Path(ref path) => path
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "ExtensionError"),
        _ => false,
    };
    if !error_ok {
        return Err(syn::Error::new_spanned(
            error,
            "Result error must be notwig_ext_api::ExtensionError",
        ));
    }
    let _ = trait_ident;
    Ok(value)
}

pub(crate) fn expand_derive(item: TokenStream) -> TokenStream {
    let input: syn::DeriveInput = syn::parse_macro_input!(item as syn::DeriveInput);
    if !input.generics.params.is_empty() {
        return syn::Error::new_spanned(
            &input.generics,
            "ExtensionCall derive does not support generic extension types yet",
        )
        .to_compile_error()
        .into();
    }
    let Some(attribute) = input
        .attrs
        .iter()
        .find(|attribute| attribute.path().is_ident("extension_call"))
    else {
        return syn::Error::new_spanned(
            &input.ident,
            "ExtensionCall derive requires #[extension_call(TraitName)]",
        )
        .to_compile_error()
        .into();
    };
    let trait_path: syn::Path = match attribute.parse_args() {
        Ok(path) => path,
        Err(error) => return error.to_compile_error().into(),
    };
    let trait_ident = trait_path
        .segments
        .last()
        .map(|segment| segment.ident.clone())
        .expect("trait path has at least one segment");
    let handle_ident = format_ident!("{}Handle", trait_ident);
    let dispatcher_ident = format_ident!("{}Dispatcher", trait_ident);
    let self_ty = &input.ident;
    quote! {
        impl ::notwig_ext_api::ExtensionCall for #self_ty {
            fn interface_id(&self) -> &'static str {
                #handle_ident::INTERFACE_ID
            }

            fn interface_version(&self) -> u32 {
                #handle_ident::INTERFACE_VERSION
            }

            fn process(
                &mut self,
                context: &::notwig_ext_api::ExtensionContext<'_>,
                method: &str,
                payload: &[u8],
            ) -> Result<Vec<u8>, ::notwig_ext_api::ExtensionError> {
                <Self as #dispatcher_ident>::dispatch_extension(
                    self,
                    context,
                    method,
                    payload,
                )
            }
        }
    }
    .into()
}
