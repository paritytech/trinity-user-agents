# truapi

_Source of truth for the TrUAPI protocol: shared traits, versioned types, and the wire dispatch table._

[![License](https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square)](../../../LICENSE)

`truapi` is the canonical Rust definition of the TrUAPI protocol. If you are changing the API surface, this crate is where it starts.

It defines:

- **Versioned data types** under `v01` and `versioned`.
- **Domain API traits** under `api/`, plus the composed `TrUApi` trait.
- **Wire ids** via trait-level `#[wire_trait(id = N)]` and per-method `#[wire(id = N)]` annotations that pin the byte-level `(trait, method)` dispatch table.
- **Subscription primitives** through `Subscription<Item, Interrupt>` for streamed host responses.
- **Authoring types** like `CallContext`, `CallError<D>`, and `CancellationToken`.

The TypeScript client and the host dispatcher are both generated from this crate.

## Architecture

The crate has two layers:

1. **Protocol types** under `v01`.
2. **Unified host contract** under `api`, where each method takes a `CallContext`, a versioned request type, and returns a versioned response with `CallError<D>` or a `Subscription<Item, CallError<D>>`.

Wire ids are part of the public protocol. Every frame carries a two-byte `(trait, method)` discriminant pair: the trait id comes from the trait-level `#[wire_trait(id = N)]` annotation and the method id from the method-level `#[wire(...)]` annotation. Trait ids and existing method ids are append-only **per trait**: never renumber or reuse an id within a trait, and never reassign a trait id. New methods take the next free method ids in their own trait without affecting any other trait. The generated Rust dispatcher and the generated TypeScript wire table must stay byte-compatible with deployed products.

## Key modules

| Module      | Role                                                                                     |
| ----------- | ---------------------------------------------------------------------------------------- |
| `v01`       | Current protocol-facing types.                                                           |
| `versioned` | Request, response, and subscription item wrappers for the unified trait surface.         |
| `api`       | Unified domain traits (`Account`, `Chain`, `Chat`, ...) and the composed `TrUApi` trait. |

Framework-level helpers (`CallError<D>`, `CallContext`, `Subscription<Item, Interrupt>`,
`CancellationToken`) live at the crate root.

## Example

Implement one or more of the unified sub-traits. `TrUApi` is a blanket trait over the full set:

```rust
use truapi::{CallContext, CallError, Subscription};
use truapi::api::{Account, TrUApi};
use truapi::latest::GenericError;
use truapi::versioned::account::{
    HostAccountConnectionStatusSubscribeError,
    HostAccountConnectionStatusSubscribeItem,
    HostAccountConnectionStatusSubscribeRequest,
    HostAccountGetError,
    HostAccountGetRequest,
    HostAccountGetResponse,
};
use truapi::v01::{self, ProductAccount};

struct MyHost;

#[truapi::async_trait]
impl Account for MyHost {
    async fn get_account(
        &self,
        _cx: &CallContext,
        _request: HostAccountGetRequest,
    ) -> Result<HostAccountGetResponse, CallError<HostAccountGetError>> {
        Ok(HostAccountGetResponse::V1(v01::HostAccountGetResponse {
            account: ProductAccount {
                public_key: Vec::new(),
            },
        }))
    }

    async fn connection_status_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostAccountConnectionStatusSubscribeRequest,
    ) -> Subscription<
        HostAccountConnectionStatusSubscribeItem,
        CallError<HostAccountConnectionStatusSubscribeError>,
    > {
        Subscription::interrupted(CallError::unavailable())
    }
}

fn _assert_truapi<T: TrUApi>() {}
```

Subscription endpoints return `Subscription<Item, Interrupt>`, a stream of `Result<Item, Interrupt>` whose first `Err`
ends it. A stream that ends without one completes normally:

```rust
use truapi::versioned::account::{
    HostAccountConnectionStatusSubscribeError, HostAccountConnectionStatusSubscribeItem,
};
use truapi::{CallError, Subscription};

fn _subscription_shape() -> Subscription<
    HostAccountConnectionStatusSubscribeItem,
    CallError<HostAccountConnectionStatusSubscribeError>,
> {
    Subscription::interrupted(CallError::unavailable())
}
```

## Used by

- [`truapi-codegen`](../truapi-codegen/) reads rustdoc JSON for this crate to generate the TypeScript client.

## License

[MIT](../../../LICENSE)
