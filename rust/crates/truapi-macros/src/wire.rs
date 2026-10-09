//! Wire-protocol metadata for TrUAPI methods.
//!
//! IDs and flags are emitted as hidden doc tags so they survive into rustdoc
//! JSON for `truapi-codegen`. Rust rejects unknown helper attributes on methods;
//! doc tags preserve the metadata without requiring such attributes.

use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, ItemFn, ItemTrait, LitInt, Token, TraitItemFn, parse_macro_input};

/// Arguments to `#[wire(...)]`.
#[derive(Default)]
pub struct WireArgs {
    /// The host starts the call and the product answers it.
    pub host_initiated: bool,
    /// Kept out of the product SDK.
    pub internal: bool,
    id: Option<u8>,
}

impl Parse for WireArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut args = WireArgs::default();

        while !input.is_empty() {
            let key: Ident = input.parse()?;

            if key == "host_initiated" {
                if args.host_initiated {
                    return Err(syn::Error::new(key.span(), "duplicate `host_initiated`"));
                }
                args.host_initiated = true;
            } else if key == "internal" {
                if args.internal {
                    return Err(syn::Error::new(key.span(), "duplicate `internal`"));
                }
                args.internal = true;
            } else {
                if key != "id" {
                    return Err(syn::Error::new(
                        key.span(),
                        "expected `id = N`, `host_initiated`, or `internal`",
                    ));
                }
                input.parse::<Token![=]>()?;
                let lit: LitInt = input.parse()?;
                let value = lit.base10_parse().map_err(|err| {
                    syn::Error::new(lit.span(), format!("wire id must fit in a u8: {err}"))
                })?;

                if args.id.replace(value).is_some() {
                    return Err(syn::Error::new(key.span(), "duplicate `id`"));
                }
            }

            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
        }

        if args.id.is_none() {
            return Err(input.error("missing `id = N`"));
        }

        Ok(args)
    }
}

/// Parse the macro input and emit generated code or a compiler diagnostic.
pub fn expand(args: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as WireArgs);
    let tags = wire_tags(&args);

    if let Ok(mut method) = syn::parse::<TraitItemFn>(item.clone()) {
        for tag in tags {
            method.attrs.push(syn::parse_quote!(#[doc = #tag]));
        }
        return quote!(#method).into();
    }

    if let Ok(mut function) = syn::parse::<ItemFn>(item) {
        for tag in tags {
            function.attrs.push(syn::parse_quote!(#[doc = #tag]));
        }
        return quote!(#function).into();
    }

    syn::Error::new(
        proc_macro2::Span::call_site(),
        "#[wire] can only be applied to trait methods or free functions",
    )
    .to_compile_error()
    .into()
}

fn wire_tags(args: &WireArgs) -> Vec<String> {
    let mut tags = Vec::new();
    if let Some(id) = args.id {
        tags.push(format!("@wire_id={id}"));
    }
    if args.host_initiated {
        tags.push("@wire_host_initiated".to_string());
    }
    if args.internal {
        tags.push("@wire_internal".to_string());
    }
    tags
}

/// Arguments to `#[wire_trait(id = N)]`.
struct WireTraitArgs {
    id: u8,
}

impl Parse for WireTraitArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let key: Ident = input.parse()?;
        if key != "id" {
            return Err(syn::Error::new(key.span(), "expected `id = N`"));
        }
        input.parse::<Token![=]>()?;
        let lit: LitInt = input.parse()?;
        let id = lit.base10_parse().map_err(|err| {
            syn::Error::new(lit.span(), format!("wire trait id must fit in a u8: {err}"))
        })?;
        if !input.is_empty() {
            return Err(input.error("expected a single `id = N` argument"));
        }
        Ok(Self { id })
    }
}

/// Parse `#[wire_trait(id = N)]` and re-emit the trait with its hidden tag.
pub fn expand_trait(args: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as WireTraitArgs);
    let tag = format!("@wire_trait_id={}", args.id);

    match syn::parse::<ItemTrait>(item) {
        Ok(mut item_trait) => {
            item_trait.attrs.push(syn::parse_quote!(#[doc = #tag]));
            quote!(#item_trait).into()
        }
        Err(_) => syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[wire_trait] can only be applied to traits",
        )
        .to_compile_error()
        .into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_methods_keep_their_wire_id_and_other_flags() {
        let args = syn::parse_str::<WireArgs>("id = 2, host_initiated, internal").unwrap();

        assert_eq!(
            wire_tags(&args),
            ["@wire_id=2", "@wire_host_initiated", "@wire_internal"]
        );
    }

    #[test]
    fn repeated_internal_flag_is_rejected() {
        let error = syn::parse_str::<WireArgs>("id = 2, internal, internal")
            .err()
            .expect("duplicate visibility must be rejected");

        assert_eq!(error.to_string(), "duplicate `internal`");
    }

    #[test]
    fn methods_are_public_unless_marked_internal() {
        let args = syn::parse_str::<WireArgs>("id = 2").unwrap();

        assert_eq!(wire_tags(&args), ["@wire_id=2"]);
    }
}
