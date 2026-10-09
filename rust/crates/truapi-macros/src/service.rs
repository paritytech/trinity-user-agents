//! Connection-scoped middleware metadata for TrUAPI service traits.

use proc_macro::TokenStream;
use quote::quote;
use syn::{
	Ident, ItemTrait, Token,
	parse::{Parse, ParseStream},
	parse_macro_input,
};

struct ServiceArgs {
	required_execution: Ident,
}

impl Parse for ServiceArgs {
	fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
		let key: Ident = input.parse()?;
		if key != "required_execution" {
			return Err(syn::Error::new(key.span(), "expected `required_execution`"));
		}
		input.parse::<Token![=]>()?;
		let required_execution = input.parse()?;
		if !input.is_empty() {
			return Err(input.error("unexpected service attribute arguments"));
		}
		Ok(Self { required_execution })
	}
}

/// Parse the macro input and emit generated code or a compiler diagnostic.
pub fn expand(args: TokenStream, item: TokenStream) -> TokenStream {
	let args = parse_macro_input!(args as ServiceArgs);
	let mut item = parse_macro_input!(item as ItemTrait);
	let tag = format!("@service_required_execution={}", args.required_execution);
	item.attrs.push(syn::parse_quote!(#[doc = #tag]));
	quote!(#item).into()
}
