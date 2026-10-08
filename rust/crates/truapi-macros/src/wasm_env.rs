//! Wasm worker bindings for a TrUAPI service trait.
//!
//! One parse of the trait yields both sides of the `truapi` wasm import
//! module, so the names a guest imports and the names a host links cannot
//! drift apart.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    Attribute, FnArg, GenericArgument, Ident, ItemTrait, PathArguments, ReturnType, TraitItem,
    TraitItemFn, Type, parse_macro_input,
};

use crate::wire::WireArgs;

/// Wasm import module every binding lives in.
const IMPORT_MODULE: &str = "truapi";

/// Whether a method answers once or streams items.
#[derive(Debug, PartialEq, Eq)]
enum Shape {
    Request,
    Subscription,
}

/// A trait method exposed to wasm guests.
struct Binding<'a> {
    method: &'a TraitItemFn,
    import_name: String,
    request: &'a Type,
    shape: Shape,
    response: &'a Type,
    error: &'a Type,
}

/// Parse the macro input and emit generated code or a compiler diagnostic.
pub fn expand(args: TokenStream, item: TokenStream) -> TokenStream {
    if !args.is_empty() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[wasm_env] takes no arguments",
        )
        .to_compile_error()
        .into();
    }
    let item_trait = parse_macro_input!(item as ItemTrait);
    match expand_trait(&item_trait) {
        Ok(bindings) => quote!(#item_trait #bindings).into(),
        Err(error) => error.to_compile_error().into(),
    }
}

fn expand_trait(item_trait: &ItemTrait) -> syn::Result<TokenStream2> {
    let trait_prefix = snake_case(&item_trait.ident.to_string());
    let mut bindings = Vec::new();
    for item in &item_trait.items {
        let TraitItem::Fn(method) = item else {
            continue;
        };
        if let Some(binding) = Binding::parse(&trait_prefix, method)? {
            bindings.push(binding);
        }
    }
    let host = expand_host(item_trait, &trait_prefix, &bindings);
    let guest = expand_guest(item_trait, &bindings);
    Ok(quote!(#host #guest))
}

impl<'a> Binding<'a> {
    /// `None` for methods a guest cannot call: internal ones and those the
    /// host starts.
    fn parse(trait_prefix: &str, method: &'a TraitItemFn) -> syn::Result<Option<Self>> {
        let Some(wire) = wire_args(&method.attrs)? else {
            return Ok(None);
        };
        if wire.internal || wire.host_initiated {
            return Ok(None);
        }
        let request = match method.sig.inputs.iter().nth(2) {
            Some(FnArg::Typed(argument)) => &*argument.ty,
            _ => {
                return Err(syn::Error::new_spanned(
                    &method.sig,
                    "expected `(&self, cx: &CallContext, request: Request)`",
                ));
            }
        };
        let (shape, response, call_error) = return_parts(&method.sig.output)?;
        let error = single_generic(call_error, "CallError")?;
        Ok(Some(Self {
            method,
            import_name: format!("{trait_prefix}_{}", method.sig.ident),
            request,
            shape,
            response,
            error,
        }))
    }
}

fn wire_args(attrs: &[Attribute]) -> syn::Result<Option<WireArgs>> {
    attrs
        .iter()
        .find(|attr| attr.path().is_ident("wire"))
        .map(|attr| attr.parse_args::<WireArgs>())
        .transpose()
}

/// Split `Result<Response, CallError<E>>` or `Subscription<Item, CallError<E>>`.
fn return_parts(output: &ReturnType) -> syn::Result<(Shape, &Type, &Type)> {
    let ReturnType::Type(_, ty) = output else {
        return Err(syn::Error::new_spanned(output, "expected a return type"));
    };
    let (ident, arguments) = last_segment(ty)?;
    let shape = match ident.to_string().as_str() {
        "Result" => Shape::Request,
        "Subscription" => Shape::Subscription,
        _ => {
            return Err(syn::Error::new_spanned(
                ty,
                "expected `Result<_, CallError<_>>` or `Subscription<_, CallError<_>>`",
            ));
        }
    };
    match arguments.as_slice() {
        [response, error] => Ok((shape, response, error)),
        _ => Err(syn::Error::new_spanned(ty, "expected two type arguments")),
    }
}

fn single_generic<'a>(ty: &'a Type, expected: &str) -> syn::Result<&'a Type> {
    match last_segment(ty)? {
        (ident, arguments) if ident == expected && arguments.len() == 1 => Ok(arguments[0]),
        _ => Err(syn::Error::new_spanned(
            ty,
            format!("expected `{expected}<_>`"),
        )),
    }
}

fn last_segment(ty: &Type) -> syn::Result<(&Ident, Vec<&Type>)> {
    let Type::Path(path) = ty else {
        return Err(syn::Error::new_spanned(ty, "expected a type path"));
    };
    let segment = path.path.segments.last().unwrap();
    let arguments = match &segment.arguments {
        PathArguments::AngleBracketed(arguments) => arguments
            .args
            .iter()
            .filter_map(|argument| match argument {
                GenericArgument::Type(ty) => Some(ty),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    Ok((&segment.ident, arguments))
}

fn expand_host(item_trait: &ItemTrait, trait_prefix: &str, bindings: &[Binding]) -> TokenStream2 {
    let trait_ident = &item_trait.ident;
    let link = format_ident!("link_{trait_prefix}");
    let doc = format!("Link the [`{trait_ident}`] methods into this environment.");
    let registrations = bindings.iter().map(|binding| {
        let name = &binding.import_name;
        let method = &binding.method.sig.ident;
        let register = match binding.shape {
            Shape::Request => quote!(request),
            Shape::Subscription => quote!(subscription),
        };
        quote! {
            self.#register(#name, |host, cx, request| {
                ::std::boxed::Box::pin(async move { #trait_ident::#method(&*host, &cx, request).await })
            });
        }
    });
    quote! {
        #[cfg(feature = "wasm-worker")]
        #[allow(deprecated)]
        impl<H: #trait_ident + 'static> crate::wasm_worker::WasmEnv<H> {
            #[doc = #doc]
            pub fn #link(&mut self) {
                #(#registrations)*
            }
        }
    }
}

fn expand_guest(item_trait: &ItemTrait, bindings: &[Binding]) -> TokenStream2 {
    let doc = format!("Wasm guest bindings for [`{}`].", item_trait.ident);
    let imports = bindings.iter().map(|binding| {
        let name = &binding.import_name;
        let method = &binding.method.sig.ident;
        quote! {
            #[link_name = #name]
            pub safe fn #method(request: *const u8, request_len: u32) -> u32;
        }
    });
    let wrappers = bindings.iter().map(|binding| {
        let docs = binding
            .method
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("doc") || attr.path().is_ident("deprecated"));
        let method = &binding.method.sig.ident;
        let request = binding.request;
        let response = binding.response;
        let error = binding.error;
        let (output, start) = match binding.shape {
            Shape::Request => (quote!(crate::guest::Call), quote!(crate::guest::Call::start)),
            Shape::Subscription => (
                quote!(crate::guest::GuestSubscription),
                quote!(crate::guest::GuestSubscription::start),
            ),
        };
        quote! {
            #(#docs)*
            pub fn #method(request: crate::latest::LatestOf<#request>) -> #output<#response, #error> {
                #start::<#request>(imports::#method, request)
            }
        }
    });
    quote! {
        #[cfg(feature = "guest")]
        #[doc = #doc]
        pub mod guest {
            use super::*;

            #[allow(unsafe_code)]
            mod imports {
                #[link(wasm_import_module = #IMPORT_MODULE)]
                unsafe extern "C" {
                    #(#imports)*
                }
            }

            #(#wrappers)*
        }
    }
}

/// `LocalStorage` to `local_storage`, matching the wire table's method prefix.
fn snake_case(ident: &str) -> String {
    let mut snake = String::new();
    for (index, character) in ident.char_indices() {
        if character.is_uppercase() && index > 0 {
            snake.push('_');
        }
        snake.push(character.to_ascii_lowercase());
    }
    snake
}

#[cfg(test)]
mod tests {
    use super::*;

    fn import_names(source: &str) -> Vec<(String, Shape)> {
        let item_trait: ItemTrait = syn::parse_str(source).unwrap();
        let prefix = snake_case(&item_trait.ident.to_string());
        item_trait
            .items
            .iter()
            .filter_map(|item| match item {
                TraitItem::Fn(method) => Binding::parse(&prefix, method).unwrap(),
                _ => None,
            })
            .map(|binding| (binding.import_name, binding.shape))
            .collect()
    }

    #[test]
    fn guests_see_public_product_initiated_methods_under_wire_table_names() {
        let names = import_names(
            "pub trait LocalStorage {
                #[wire(id = 0)]
                async fn read(&self, cx: &CallContext, request: ReadRequest)
                    -> Result<ReadResponse, CallError<ReadError>>;
                #[wire(id = 1)]
                async fn subscribe(&self, cx: &CallContext, request: SubscribeRequest)
                    -> Subscription<Item, CallError<SubscribeError>>;
                #[wire(id = 2, internal)]
                async fn authorize(&self, cx: &CallContext, request: AuthorizeRequest)
                    -> Result<AuthorizeResponse, CallError<AuthorizeError>>;
                #[wire(id = 3, host_initiated)]
                async fn render(&self, cx: &CallContext, request: RenderRequest)
                    -> Subscription<Node, CallError<RenderError>>;
            }",
        );

        assert_eq!(
            names,
            [
                ("local_storage_read".to_string(), Shape::Request),
                ("local_storage_subscribe".to_string(), Shape::Subscription),
            ]
        );
    }
}
