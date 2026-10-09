//! Versioned message envelopes and their conversion trait implementations.

use proc_macro::TokenStream;
use proc_macro2::Literal;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Attribute, Ident, Token, Type, Visibility, braced, parse_macro_input};

/// One sequence of versioned envelope declarations passed to `versioned_type!`.
struct VersionedInput {
    enums: Vec<VersionedEnum>,
}

impl Parse for VersionedInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut enums = Vec::new();
        while !input.is_empty() {
            enums.push(input.parse()?);
        }
        Ok(Self { enums })
    }
}

/// A single `[vis] enum Name { V1 => Ty, ... }` declaration.
struct VersionedEnum {
    attrs: Vec<Attribute>,
    vis: Visibility,
    name: Ident,
    variants: Vec<VersionedVariant>,
}

impl Parse for VersionedEnum {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let vis: Visibility = input.parse()?;
        input.parse::<Token![enum]>()?;
        let name: Ident = input.parse()?;

        let body;
        braced!(body in input);
        let mut variants = Vec::new();
        while !body.is_empty() {
            variants.push(body.parse()?);
            if body.peek(Token![,]) {
                body.parse::<Token![,]>()?;
            } else {
                break;
            }
        }

        Ok(Self {
            attrs,
            vis,
            name,
            variants,
        })
    }
}

/// A single `Vn` or `Vn => Ty` variant.
struct VersionedVariant {
    attrs: Vec<Attribute>,
    ident: Ident,
    ty: Option<Type>,
}

impl Parse for VersionedVariant {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let ident: Ident = input.parse()?;
        let ty = if input.peek(Token![=>]) {
            input.parse::<Token![=>]>()?;
            Some(input.parse()?)
        } else {
            None
        };
        Ok(Self { attrs, ident, ty })
    }
}

/// True when `attrs` already carries a doc comment or `#[doc]` attribute.
fn has_doc(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident("doc"))
}

/// Parse the `Vn` version number from a variant identifier.
fn variant_version(ident: &Ident) -> syn::Result<u8> {
    let name = ident.to_string();
    let err = || syn::Error::new(ident.span(), "variant must be named `Vn` where n is a u8");
    name.strip_prefix('V')
        .ok_or_else(err)?
        .parse::<u8>()
        .map_err(|_| err())
}

/// Parse the macro input and emit generated code or a compiler diagnostic.
pub fn expand(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as VersionedInput);
    match expand_versioned(&input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn expand_versioned(input: &VersionedInput) -> syn::Result<proc_macro2::TokenStream> {
    let mut out = proc_macro2::TokenStream::new();
    for enum_def in &input.enums {
        out.extend(expand_versioned_enum(enum_def)?);
    }
    Ok(out)
}

fn expand_versioned_enum(def: &VersionedEnum) -> syn::Result<proc_macro2::TokenStream> {
    let VersionedEnum {
        attrs,
        vis,
        name,
        variants,
    } = def;

    if variants.is_empty() {
        return Err(syn::Error::new(
            name.span(),
            "versioned enum needs at least one variant",
        ));
    }

    let mut variant_defs = Vec::new();
    let mut version_arms = Vec::new();
    for (i, variant) in variants.iter().enumerate() {
        let expected = i + 1;
        let version = variant_version(&variant.ident)?;
        if usize::from(version) != expected {
            return Err(syn::Error::new(
                variant.ident.span(),
                format!("expected variant `V{expected}`; versions must be contiguous from 1"),
            ));
        }

        let index = Literal::u8_unsuffixed(i as u8);
        let version_lit = Literal::u8_unsuffixed(version);
        let vattrs = &variant.attrs;
        let vident = &variant.ident;
        let default_doc = (!has_doc(vattrs)).then(|| {
            let doc = match &variant.ty {
                Some(_) => format!("Version {version} payload."),
                None => format!("Version {version} (no payload)."),
            };
            quote! { #[doc = #doc] }
        });
        match &variant.ty {
            Some(ty) => {
                variant_defs.push(
                    quote! { #(#vattrs)* #default_doc #[codec(index = #index)] #vident(#ty) },
                );
                version_arms.push(quote! { Self::#vident(..) => #version_lit });
            }
            None => {
                variant_defs
                    .push(quote! { #(#vattrs)* #default_doc #[codec(index = #index)] #vident });
                version_arms.push(quote! { Self::#vident => #version_lit });
            }
        }
    }

    let doc = format!("Versioned envelope for [`{name}`].");
    let latest_lit = Literal::u8_unsuffixed(variants.len() as u8);
    let newest = variants.last().expect("checked non-empty");
    let newest_ident = &newest.ident;
    let (latest_ty, wrap_param, wrap_body) = match &newest.ty {
        Some(ty) => (
            quote! { #ty },
            quote! { latest },
            quote! { Self::#newest_ident(latest) },
        ),
        None => (
            quote! { () },
            quote! { _latest },
            quote! { Self::#newest_ident },
        ),
    };

    let mut tokens = quote! {
        #(#attrs)*
        #[doc = #doc]
        #[derive(Debug, Clone, PartialEq, Eq, parity_scale_codec::Encode, parity_scale_codec::Decode)]
        #vis enum #name {
            #(#variant_defs),*
        }

        impl crate::versioned::Versioned for #name {
            type Latest = #latest_ty;
            const LATEST: u8 = #latest_lit;
            fn version(&self) -> u8 {
                match self {
                    #(#version_arms),*
                }
            }
            fn wrap_latest(#wrap_param: Self::Latest) -> Self {
                #wrap_body
            }
        }
    };

    if let [only] = &variants[..] {
        let vident = &only.ident;
        let (into_body, from_param, from_body) = match &only.ty {
            Some(_) => (
                quote! { match self { Self::#vident(inner) => inner } },
                quote! { latest },
                quote! { Self::#vident(latest) },
            ),
            None => (
                quote! { match self { Self::#vident => () } },
                quote! { _latest },
                quote! { Self::#vident },
            ),
        };
        tokens.extend(quote! {
            impl crate::versioned::IntoLatest for #name {
                fn into_latest(self) -> Self::Latest {
                    #into_body
                }
            }

            impl crate::versioned::FromLatest for #name {
                fn from_latest(#from_param: Self::Latest, _target: u8) -> Self {
                    #from_body
                }
            }
        });
    }

    Ok(tokens)
}
