//! Entry point of a product worker compiled to wasm.

use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemFn, parse_macro_input};

/// Parse the macro input and emit generated code or a compiler diagnostic.
pub fn expand(args: TokenStream, item: TokenStream) -> TokenStream {
    if !args.is_empty() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[truapi_guest_api::main] takes no arguments",
        )
        .to_compile_error()
        .into();
    }
    let entry = parse_macro_input!(item as ItemFn);
    match check(&entry) {
        Ok(()) => {
            let ident = &entry.sig.ident;
            quote! {
                #entry
                ::truapi_guest_api::export_entry!(#ident);
            }
            .into()
        }
        Err(error) => error.to_compile_error().into(),
    }
}

fn check(entry: &ItemFn) -> syn::Result<()> {
    if entry.sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            entry.sig.fn_token,
            "the worker entry point must be an `async fn`",
        ));
    }
    if !entry.sig.inputs.is_empty() || !entry.sig.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &entry.sig,
            "the worker entry point takes no arguments or generics",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error(source: &str) -> Option<String> {
        check(&syn::parse_str(source).unwrap())
            .err()
            .map(|error| error.to_string())
    }

    #[test]
    fn only_an_argumentless_async_fn_can_be_the_entry_point() {
        assert_eq!(
            [
                error("async fn main() -> Result<(), Error> { Ok(()) }"),
                error("fn main() -> Result<(), Error> { Ok(()) }"),
                error("async fn main(product: String) -> Result<(), Error> { Ok(()) }"),
            ],
            [
                None,
                Some("the worker entry point must be an `async fn`".to_string()),
                Some("the worker entry point takes no arguments or generics".to_string()),
            ]
        );
    }
}
