//! The product protocol and its full wasm worker environment.

use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemTrait, TypeParamBound, parse_macro_input};

use crate::wasm_env;

/// Generate the full product environment from the service bounds of the product trait.
pub fn expand(args: TokenStream, item: TokenStream) -> TokenStream {
	parse_macro_input!(args as syn::parse::Nothing);
	let product = parse_macro_input!(item as ItemTrait);
	let contract = &product.ident;
	let bounds = &product.supertraits;
	let mut links = Vec::new();
	for bound in bounds {
		let TypeParamBound::Trait(service) = bound else {
			return syn::Error::new_spanned(bound, "expected a service trait")
				.to_compile_error()
				.into();
		};
		let service = &service.path.segments.last().unwrap().ident;
		links.push(wasm_env::link_method(service));
	}
	quote! {
		#product

		impl<Host: #bounds> #contract for Host {}

		#[cfg(all(feature = "wasm-worker", not(target_arch = "wasm32")))]
		impl<Host: #contract + ?Sized + 'static> crate::wasm_worker::WasmEnv<Host> {
			/// Link every product-callable method of the protocol to this host.
			pub fn for_product(host: ::std::sync::Arc<Host>) -> Self {
				let mut env = Self::new(host);
				#(env.#links();)*
				env
			}
		}
	}
	.into()
}
