# TrUAPI proc macros

This crate provides TrUAPI wire annotations and versioned envelopes, plus
a server-specific macro for inter-host SSO contracts.

Each macro has its own implementation module. [`lib.rs`](src/lib.rs) contains
the thin public entry points, which Rust requires at the proc-macro crate root.

| Macro | Input | Generated code |
| --- | --- | --- |
| [`service`](src/service.rs) | TrUAPI service trait | Required middleware metadata for codegen |
| [`wire`](src/wire.rs) | TrUAPI method | Wire IDs and flags for codegen |
| [`versioned_type!`](src/versioned_type.rs) | Versioned envelope declarations | SCALE enums and version conversion traits |
| [`sso_service`](src/sso_service.rs) | Dedicated inherent impl of SSO handlers | Request/response conversions, exhaustive dispatch, and message naming/correlation helpers |

## Handler contract

Every method in the annotated impl is an endpoint. Its name selects a wire
request variant, its parameter declares the payload, and its return type names
the response's `Result` payload:

```rust
pub type GetAccountAliasResponse = Result<HostAccountGetAliasResponse, RingVrfError>;

#[truapi_macros::sso_service]
impl SigningHostSsoService {
    async fn get_account_alias(
        &self,
        cx: &SsoRequestContext,
        request: ProductRequest<HostAccountGetAliasRequest>,
    ) -> GetAccountAliasResponse {
        self.signing_host
            .account_alias(&cx.call, &cx.session, request)
            .await
    }
}
```

The method `get_account_alias` selects `GetAccountAliasRequest`; parameter types
can be canonical payloads or generic wrappers without request aliases.
The return type's name selects the wire response variant. For example,
`create_transaction` and `create_transaction_with_legacy_account` both return
`CreateTransactionResponse`, so request and response stems need not match. Distinct variants may
carry identical result types; conversion belongs to the request, so those
responses remain distinguishable. Constructors and helpers belong in a separate impl.

Handler signatures define the protocol pairing. Changing a selected response
variant is a protocol change even when its Rust payload type matches another
variant. The compiler checks coverage and payload compatibility; it cannot infer
the intended operation from structurally identical types.

Handler signatures expand to native async methods returning `SsoReply<Payload>`.
Bodies return the named `Result` or an explicit reply with a local transcript
outcome. Shared Rust code adds `Response<P> { responding_to, payload }`;
the generated request contract selects its wire variant. An inner async block
preserves `?` and early returns.

The generated `dispatch(&self, cx, message)` method matches the existing
wire enum directly, routing requests to handlers and handling responses,
disconnects and cancels separately. The caller builds `cx` for the message, so
it owns the request's cancellation. Without one it returns the response's typed
disconnected error.
Shared reply finishing supplies correlation and defaults the transcript outcome
to success or error; handlers classify operation-specific outcomes. Missing
handlers, undeclared wire variants, and incompatible payloads fail compilation.

The wire enum contains requests, responses, disconnects and cancels in one
SCALE tag space. Dispatch has no catch-all arm: every request needs a handler,
and every response must be selected by at least one handler. Handler parameters
must match their wire payloads, including any boxing. The macro also generates
the enum's `name()`, `responding_to()`, and `with_responding_to()` helpers.

## Server integration

This macro targets contracts in `crate::host_logic::sso::{messages, wire}` and
`crate::runtime::{authority, sso_service}`. It is intended for invocation inside
the `truapi` runtime. Wire encoding remains owned by the enum and payload
codec derives. Transport, consent, session revalidation, and business logic belong
to the server implementation.

The [compiler tests](tests/sso.rs) exercise the macro against minimal versions of
those contracts. They cover valid handlers, shared and explicit response pairing,
boxed payloads, wire helpers, and invalid declarations. The `.stderr` snapshots
track CI's current stable rustc diagnostics; older compilers can report the same
errors with different wording. Run them with an up-to-date stable toolchain:

```sh
rustup update stable
cargo +stable test -p truapi-macros --locked
```
