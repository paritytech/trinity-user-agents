//! Proc macros for TrUAPI annotations, versioned envelopes, and inter-host SSO.
//!
//! Each macro's implementation lives in its own module. Rust requires the
//! public proc-macro entry points to be defined at the crate root.

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
#[proc_macro_attribute]
pub fn wire_trait(args: TokenStream, item: TokenStream) -> TokenStream {
    wire::expand_trait(args, item)
}

/// Expose a TrUAPI service trait to wasm workers.
///
/// ```ignore
/// #[wasm_env]
/// #[wire_trait(id = 2)]
/// #[crate::async_trait]
/// pub trait Account: Send + Sync { ... }
/// ```
///
/// Must precede `#[wire_trait]` and `#[async_trait]`, so it reads the
/// methods as written. Every method a product may start becomes an import
/// named `<trait>_<method>` in the `truapi` wasm module, the same name the
/// wire table uses. From that one list the macro emits:
///
/// - under the `wasm-worker` feature, `WasmEnv::link_<trait>`, which links
///   each import to a typed call on the product's trait implementation;
/// - under the `guest` feature, a `guest` module beside the trait with the
///   import declarations and one typed wrapper per method.
///
/// Only works inside `truapi`.
#[proc_macro_attribute]
pub fn wasm_env(args: TokenStream, item: TokenStream) -> TokenStream {
    wasm_env::expand(args, item)
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
/// `impl Versioned` exposing `Latest`, `LATEST`, `version()`, and
/// `wrap_latest()`. Single-version envelopes also get trivial
/// `IntoLatest`/`FromLatest` impls; multi-version envelopes leave those to be
/// written by hand, since the conversion is bespoke.
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
