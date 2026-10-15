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

/// Which side starts the calls of a method.
#[derive(Debug, PartialEq, Eq)]
enum Initiator {
	/// The guest starts the call through an import.
	Product,
	/// The host starts the call, and the guest answers it through a handler.
	Host,
}

/// A trait method exposed to wasm guests.
struct Binding<'a> {
	method: &'a TraitItemFn,
	import_name: String,
	initiator: Initiator,
	request: &'a Type,
	shape: Shape,
	response: &'a Type,
	error: &'a Type,
}

/// Parse the macro input and emit generated code or a compiler diagnostic.
pub fn expand(args: TokenStream, item: TokenStream) -> TokenStream {
	if !args.is_empty() {
		return syn::Error::new(proc_macro2::Span::call_site(), "#[wasm_env] takes no arguments")
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
	let mut handlers = Vec::new();
	let mut excluded = Vec::new();
	for item in &item_trait.items {
		let TraitItem::Fn(method) = item else {
			continue;
		};
		match Binding::parse(&trait_prefix, method)? {
			Some(binding) if binding.initiator == Initiator::Product => bindings.push(binding),
			Some(binding) => {
				excluded.push(binding.import_name.clone());
				handlers.push(binding);
			},
			None if wire_args(&method.attrs)?.is_some() => {
				excluded.push(format!("{trait_prefix}_{}", method.sig.ident));
			},
			None => {},
		}
	}
	let host = expand_host(item_trait, &trait_prefix, &bindings, &excluded);
	let guest = expand_guest(item_trait, &bindings, &handlers);
	Ok(quote!(#host #guest))
}

impl<'a> Binding<'a> {
	/// `None` for methods a guest neither calls nor answers: the internal
	/// ones.
	fn parse(trait_prefix: &str, method: &'a TraitItemFn) -> syn::Result<Option<Self>> {
		let Some(wire) = wire_args(&method.attrs)? else {
			return Ok(None);
		};
		if wire.internal {
			return Ok(None);
		}
		let initiator = if wire.host_initiated { Initiator::Host } else { Initiator::Product };
		let request = match method.sig.inputs.iter().nth(2) {
			Some(FnArg::Typed(argument)) => &*argument.ty,
			_ => {
				return Err(syn::Error::new_spanned(
					&method.sig,
					"expected `(&self, cx: &CallContext, request: Request)`",
				));
			},
		};
		let (shape, response, call_error) = return_parts(&method.sig.output)?;
		if initiator == Initiator::Host && shape != Shape::Subscription {
			return Err(syn::Error::new_spanned(
				&method.sig.output,
				"a host-initiated method must return `Subscription<_, CallError<_>>`",
			));
		}
		let error = single_generic(call_error, "CallError")?;
		Ok(Some(Self {
			method,
			import_name: format!("{trait_prefix}_{}", method.sig.ident),
			initiator,
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
		},
	};
	match arguments.as_slice() {
		[response, error] => Ok((shape, response, error)),
		_ => Err(syn::Error::new_spanned(ty, "expected two type arguments")),
	}
}

fn single_generic<'a>(ty: &'a Type, expected: &str) -> syn::Result<&'a Type> {
	match last_segment(ty)? {
		(ident, arguments) if ident == expected && arguments.len() == 1 => Ok(arguments[0]),
		_ => Err(syn::Error::new_spanned(ty, format!("expected `{expected}<_>`"))),
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

fn expand_host(
	item_trait: &ItemTrait,
	trait_prefix: &str,
	bindings: &[Binding],
	excluded: &[String],
) -> TokenStream2 {
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
		#[cfg(all(feature = "wasm-worker", not(target_arch = "wasm32")))]
		#[allow(deprecated)]
		impl<H: #trait_ident + 'static> crate::wasm_worker::WasmEnv<H> {
			#[doc = #doc]
			pub fn #link(&mut self) {
				#(#registrations)*
				#(self.exclude(#excluded);)*
			}
		}
	}
}

fn expand_guest(
	item_trait: &ItemTrait,
	bindings: &[Binding],
	handlers: &[Binding],
) -> TokenStream2 {
	let doc = format!("Wasm guest bindings for [`{}`].", item_trait.ident);
	let imports = bindings.iter().map(|binding| {
		let name = &binding.import_name;
		let method = &binding.method.sig.ident;
		let shape = match binding.shape {
			Shape::Request => "crate::guest::RawCall",
			Shape::Subscription => "crate::guest::RawSubscription",
		};
		let doc = format!(
			"Import `{name}`: start the call with an encoded versioned request. Give it to [`{shape}::start`]."
		);
		quote! {
			#[doc = #doc]
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
	let registrations = handlers.iter().map(|binding| {
		let name = &binding.import_name;
		let register = format_ident!("on_{}", binding.method.sig.ident);
		let request = binding.request;
		let item = binding.response;
		let error = binding.error;
		let doc = format!(
			"Answer each `{name}` call that the host starts with `handler`. The handler gets the \
			 latest request and the sink of the call. `Ok(())` completes the call, and an error \
			 interrupts it. Register it before the first `.await` of the entry point, so that no \
			 call comes before it; a call with no handler ends with `Unsupported`."
		);
		quote! {
			#[doc = #doc]
			pub fn #register<Handler, Answer>(handler: Handler)
			where
				Handler: Fn(crate::latest::LatestOf<#request>, crate::guest::Sink<#item>) -> Answer
					+ 'static,
				Answer: ::core::future::Future<
						Output = ::core::result::Result<
							(),
							crate::CallError<crate::latest::LatestOf<#error>>,
						>,
					> + 'static,
			{
				crate::guest::on_call::<#request, #item, #error, Handler, Answer>(#name, handler)
			}
		}
	});
	quote! {
		#[cfg(all(feature = "guest", target_arch = "wasm32"))]
		#[doc = #doc]
		pub mod guest {
			use super::*;

			/// The raw imports of the methods, one [`crate::guest::StartImport`]
			/// for each method.
			#[allow(unsafe_code)]
			pub mod imports {
				#[link(wasm_import_module = #IMPORT_MODULE)]
				unsafe extern "C" {
					#(#imports)*
				}
			}

			#(#wrappers)*

			#(#registrations)*
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

	fn import_names(source: &str) -> Vec<(String, Initiator, Shape)> {
		let item_trait: ItemTrait = syn::parse_str(source).unwrap();
		let prefix = snake_case(&item_trait.ident.to_string());
		item_trait
			.items
			.iter()
			.filter_map(|item| match item {
				TraitItem::Fn(method) => Binding::parse(&prefix, method).unwrap(),
				_ => None,
			})
			.map(|binding| (binding.import_name, binding.initiator, binding.shape))
			.collect()
	}

	#[test]
	fn guests_call_and_answer_the_public_methods_under_wire_table_names() {
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
				("local_storage_read".to_string(), Initiator::Product, Shape::Request),
				("local_storage_subscribe".to_string(), Initiator::Product, Shape::Subscription),
				("local_storage_render".to_string(), Initiator::Host, Shape::Subscription),
			]
		);
	}

	#[test]
	fn a_host_initiated_method_must_stream_its_answer() {
		let item_trait: ItemTrait = syn::parse_str(
			"pub trait Renderer {
                #[wire(id = 0, host_initiated)]
                async fn render(&self, cx: &CallContext, request: RenderRequest)
                    -> Result<RenderResponse, CallError<RenderError>>;
            }",
		)
		.unwrap();
		let TraitItem::Fn(method) = &item_trait.items[0] else { unreachable!() };

		let error = Binding::parse("renderer", method).err().map(|error| error.to_string());

		assert_eq!(
			error.as_deref(),
			Some("a host-initiated method must return `Subscription<_, CallError<_>>`")
		);
	}
}
