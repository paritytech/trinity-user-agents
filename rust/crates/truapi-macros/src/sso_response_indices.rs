//! Identify response prefixes from the same catalog that owns SCALE encoding.

use quote::quote;
use syn::{Fields, ItemEnum, Type, parse_macro_input};

pub fn expand(
    args: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    if !args.is_empty() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "sso_response_indices takes no arguments",
        )
        .to_compile_error()
        .into();
    }
    let item = parse_macro_input!(item as ItemEnum);
    match response_indices(&item) {
        Ok(indices) => {
            let name = &item.ident;
            quote! {
                #item
                impl #name {
                    /// Whether this SCALE discriminant starts a `Response<P>` envelope.
                    pub(super) const fn is_response_index(index: u8) -> bool {
                        matches!(index, #(#indices)|*)
                    }
                }
            }
            .into()
        }
        Err(error) => error.to_compile_error().into(),
    }
}

fn response_indices(item: &ItemEnum) -> syn::Result<Vec<u8>> {
    let mut indices = Vec::new();
    for (ordinal, variant) in item.variants.iter().enumerate() {
        let mut index = u8::try_from(ordinal).map_err(|_| {
            syn::Error::new_spanned(variant, "SSO discriminants must fit in one byte")
        })?;
        if variant.discriminant.is_some() {
            return Err(syn::Error::new_spanned(
                variant,
                "use codec(index = ...) for SSO discriminants",
            ));
        }
        for attribute in variant
            .attrs
            .iter()
            .filter(|attribute| attribute.path().is_ident("codec"))
        {
            attribute.parse_nested_meta(|meta| {
                if !meta.path.is_ident("index") {
                    return Err(meta.error("only codec(index = ...) is supported for SSO variants"));
                }
                index = meta.value()?.parse::<syn::LitInt>()?.base10_parse()?;
                Ok(())
            })?;
        }
        let Fields::Unnamed(fields) = &variant.fields else {
            continue;
        };
        if fields.unnamed.len() != 1 {
            continue;
        }
        let Type::Path(payload) = &fields.unnamed[0].ty else {
            continue;
        };
        if payload
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "Response")
        {
            indices.push(index);
        }
    }
    if indices.is_empty() {
        return Err(syn::Error::new_spanned(
            item,
            "SSO messages must contain Response<P> variants",
        ));
    }
    Ok(indices)
}
