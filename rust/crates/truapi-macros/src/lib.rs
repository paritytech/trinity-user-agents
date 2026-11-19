//! Proc macros for TrUAPI annotations, versioned envelopes, and inter-host SSO.
//!
//! Each macro's implementation lives in its own module. Rust requires the
//! public proc-macro entry points to be defined at the crate root.

mod guest_main;
mod product_services;
mod service;
mod sso_service;
mod versioned_type;
mod wasm_env;
mod wire;

use proc_macro::TokenStream;

/// Declare connection-scoped middleware required by a TrUAPI service trait.
///
/// The metadata is preserved in rustdoc JSON for `truapi-codegen`.
#[proc_macro_attribute]
pub fn service(args: TokenStream, item: TokenStream) -> TokenStream {
	service::expand(args, item)
}

/// Mark a TrUAPI trait method with its wire-protocol discriminant id.
///
/// ```ignore
/// #[wire(id = 4)]
/// async fn host_account_get(...) -> ...;
///
/// #[wire(id = 42)]
/// async fn host_account_connection_status_subscribe(...) -> ...;
/// ```
///
/// One id addresses the method regardless of shape (request/response or
/// subscription): which leg a frame carries is named by its `message_type`
/// byte, not by a separate wire id.
///
/// `#[wire(id = N, internal)]` keeps a method out of the generated product
/// SDK while retaining host dispatch. It does not authenticate the caller.
///
/// Expands to the original method plus hidden doc tags that `truapi-codegen`
/// extracts from rustdoc JSON to build the wire table and versioned clients.
#[proc_macro_attribute]
pub fn wire(args: TokenStream, item: TokenStream) -> TokenStream {
	wire::expand(args, item)
}

/// Mark a TrUAPI service trait with its wire-protocol trait discriminant.
///
/// ```ignore
/// #[wire_trait(id = 1)]
/// pub trait System: Send + Sync { ... }
/// ```
///
/// The trait id is the first byte of the `(trait, method)` discriminant pair
/// every frame of the trait's methods carries on the wire. Expands to the
/// original trait plus a hidden `@wire_trait_id=N` doc tag that
/// `truapi-codegen` extracts from rustdoc JSON.
///
/// Inside `truapi`, this also generates a typed host linker for each service on native builds
/// with `wasm-worker` and wrappers for its product-callable methods on wasm32 with `guest`.
#[proc_macro_attribute]
pub fn wire_trait(args: TokenStream, item: TokenStream) -> TokenStream {
	wire::expand_trait(args, item)
}

/// Make an `async fn` the entry point of a product worker compiled to wasm.
///
/// ```ignore
/// #[truapi::main]
/// async fn main() -> Result<(), truapi::Error> {
///     Ok(())
/// }
/// ```
///
/// Emits the exports the host starts the worker and delivers events through.
/// Re-exported as `truapi_guest_api::main`; the expansion finds that crate
/// under whatever name the worker depends on it by.
#[proc_macro_attribute]
pub fn guest_main(args: TokenStream, item: TokenStream) -> TokenStream {
	guest_main::expand(args, item)
}

/// Generate the blanket protocol implementation and the worker environment for a set of services.
///
/// Apply to the product trait whose supertraits are its service traits. On native hosts with
/// `wasm-worker`, `WasmEnv::for_product` links each service's product-callable methods.
#[proc_macro_attribute]
pub fn product_services(args: TokenStream, item: TokenStream) -> TokenStream {
	product_services::expand(args, item)
}

/// Generate versioned message envelopes.
///
/// ```ignore
/// versioned_type! {
///     pub enum HostFooRequest { V1 => v01::HostFooRequest }
///     pub enum HostFooResponse { V1 }
/// }
/// ```
///
/// Each declaration becomes a SCALE enum with positional codec indices and an
/// `impl Versioned` exposing `Latest`, `LATEST`, and `version()`. Converting
/// the latest payload with `From` produces the newest variant, including when
/// its payload is `()`. Single-version envelopes also implement `IntoLatest`
/// and `FromLatest`. Multi-version envelopes provide their own conversions.
///
/// The enum and every variant receive generated doc comments; a variant keeps
/// its own doc attributes when the declaration provides them.
///
/// The declared visibility (`pub`, or none) carries through to the
/// generated enum.
///
/// The generated impls name `crate::versioned::*` traits, so invoke this from
/// within the `truapi` crate.
#[proc_macro]
pub fn versioned_type(item: TokenStream) -> TokenStream {
	versioned_type::expand(item)
}

/// Define SSO handlers in a dedicated inherent implementation.
///
/// Every method must be `async fn name(&self, cx: &SsoRequestContext, request:
/// <Request>) -> <Response>`, where the named response aliases its `Result`
/// payload and selects the wire variant. The method name selects the request
/// variant; the parameter must match its payload type, which can be generic.
/// Each signature supplies `SsoRequest` pairing. The macro generates `dispatch`
/// on the service and naming/correlation helpers on the existing wire enum.
/// Dispatch matches the wire enum exhaustively, so every request needs a handler
/// and every response must be selected by a handler. Multiple handlers may share
/// a response variant. Wire encoding remains owned by the codec derives.
/// Handler return types expand to `SsoReply<Payload>`, and bodies return
/// ordinary `Result` payloads or explicit replies with a transcript outcome.
/// An inner async block preserves `return` and `?` semantics. Constructors and
/// other helpers belong in a separate, unannotated implementation.
///
/// The macro uses native async methods and refers to the context and reply
/// types in `crate::runtime::sso_service`. It only works inside `truapi`.
#[proc_macro_attribute]
pub fn sso_service(args: TokenStream, item: TokenStream) -> TokenStream {
	sso_service::expand(args, item)
}
